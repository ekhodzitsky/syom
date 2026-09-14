/* Isolated FFmpeg 9.0.1 native AAC adapter. Not part of the syom crate. */

#include "avc_adapt.h"

#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/channel_layout.h>
#include <libavutil/opt.h>
#include <libswresample/swresample.h>

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

const char *avc_engine_id(void) { return "ffmpeg-9.0.1-native-aac threads=1"; }

static uint64_t fnv1a_f32le(uint64_t h, const float *p, size_t n) {
    const uint64_t prime = 0x100000001b3ULL;
    size_t i;
    int b;
    for (i = 0; i < n; i++) {
        uint32_t bits;
        memcpy(&bits, &p[i], 4);
        for (b = 0; b < 4; b++) {
            h ^= (uint64_t)((bits >> (8 * b)) & 0xffu);
            h *= prime;
        }
    }
    return h;
}

static int adts_frame_len(const uint8_t *p, size_t n) {
    int len;
    if (n < 7)
        return -1;
    if (p[0] != 0xff || (p[1] & 0xf0) != 0xf0)
        return -1;
    len = ((p[3] & 0x03) << 11) | (p[4] << 3) | (p[5] >> 5);
    if (len < 7 || (size_t)len > n)
        return -1;
    return len;
}

static int fail(AvcPcm *out, const char *msg) {
    if (out) {
        memset(out, 0, sizeof(*out));
        snprintf(out->error, sizeof(out->error), "%s", msg);
    }
    return -1;
}

static int append_fltp(AVFrame *fr, float ***planes, int *ch, int *cap, int *got,
                       uint32_t *rate) {
    int c, n = fr->nb_samples;
    int i;
    if (*ch == 0) {
        *ch = fr->ch_layout.nb_channels;
        *rate = (uint32_t)fr->sample_rate;
        if (*ch <= 0 || *ch > 8)
            return -1;
        *planes = calloc((size_t)*ch, sizeof(float *));
        if (!*planes)
            return -1;
    }
    if (fr->ch_layout.nb_channels != *ch || (uint32_t)fr->sample_rate != *rate)
        return -1;
    if (*got + n > *cap) {
        int ncap = (*cap == 0) ? 8192 : *cap;
        while (ncap < *got + n)
            ncap *= 2;
        for (c = 0; c < *ch; c++) {
            float *p = realloc((*planes)[c], (size_t)ncap * sizeof(float));
            if (!p)
                return -1;
            (*planes)[c] = p;
        }
        *cap = ncap;
    }
    if (fr->format == AV_SAMPLE_FMT_FLTP) {
        for (c = 0; c < *ch; c++)
            memcpy((*planes)[c] + *got, fr->extended_data[c], (size_t)n * sizeof(float));
    } else if (fr->format == AV_SAMPLE_FMT_FLT) {
        const float *inter = (const float *)fr->extended_data[0];
        for (i = 0; i < n; i++)
            for (c = 0; c < *ch; c++)
                (*planes)[c][*got + i] = inter[i * *ch + c];
    } else if (fr->format == AV_SAMPLE_FMT_S16) {
        const int16_t *inter = (const int16_t *)fr->extended_data[0];
        for (i = 0; i < n; i++)
            for (c = 0; c < *ch; c++)
                (*planes)[c][*got + i] = (float)inter[i * *ch + c] / 32768.f;
    } else if (fr->format == AV_SAMPLE_FMT_S16P) {
        for (c = 0; c < *ch; c++) {
            const int16_t *src = (const int16_t *)fr->extended_data[c];
            for (i = 0; i < n; i++)
                (*planes)[c][*got + i] = (float)src[i] / 32768.f;
        }
    } else {
        return -2;
    }
    *got += n;
    return 0;
}

static void free_planes(float **planes, int ch) {
    int c;
    if (!planes)
        return;
    for (c = 0; c < ch; c++)
        free(planes[c]);
    free(planes);
}

static int finish_pcm(AvcPcm *out, float **planes, int ch, int got, uint32_t rate,
                      size_t consumed, const char *lane) {
    uint64_t h = 0xcbf29ce484222325ULL;
    int c;
    memset(out, 0, sizeof(*out));
    snprintf(out->lane, sizeof(out->lane), "%s", lane);
    if (ch <= 0 || got <= 0) {
        snprintf(out->error, sizeof(out->error), "empty pcm");
        return -1;
    }
    out->sample_rate = rate;
    out->channels = (uint32_t)ch;
    out->samples = (uint64_t)got;
    out->consumed = consumed;
    for (c = 0; c < ch; c++)
        h = fnv1a_f32le(h, planes[c], (size_t)got);
    out->checksum = h;
    return 0;
}

static int drain_decode(AVCodecContext *ctx, AVFrame *fr, float ***planes, int *ch,
                        int *cap, int *got, uint32_t *rate) {
    for (;;) {
        int r = avcodec_receive_frame(ctx, fr);
        if (r == AVERROR(EAGAIN) || r == AVERROR_EOF)
            return 0;
        if (r < 0)
            return -1;
        if (append_fltp(fr, planes, ch, cap, got, rate) < 0)
            return -1;
        av_frame_unref(fr);
    }
}

int avc_decode_au(const uint8_t *data, size_t len, AvcPcm *out) {
    const AVCodec *dec;
    AVCodecContext *ctx = NULL;
    AVDictionary *opts = NULL;
    AVPacket *pkt = NULL;
    AVFrame *fr = NULL;
    float **planes = NULL;
    int ch = 0, cap = 0, got = 0, rc = -1;
    uint32_t rate = 0;
    size_t pos = 0, consumed = 0;

    memset(out, 0, sizeof(*out));
    snprintf(out->lane, sizeof(out->lane), "in_process_au");
    dec = avcodec_find_decoder(AV_CODEC_ID_AAC);
    if (!dec)
        return fail(out, "aac decoder missing");
    ctx = avcodec_alloc_context3(dec);
    pkt = av_packet_alloc();
    fr = av_frame_alloc();
    if (!ctx || !pkt || !fr) {
        fail(out, "alloc");
        goto done;
    }
    av_dict_set(&opts, "threads", "1", 0);
    if (avcodec_open2(ctx, dec, &opts) < 0) {
        fail(out, "avcodec_open2");
        goto done;
    }
    while (pos < len) {
        int fl = adts_frame_len(data + pos, len - pos);
        if (fl < 0)
            break;
        pkt->data = (uint8_t *)(data + pos);
        pkt->size = fl;
        if (avcodec_send_packet(ctx, pkt) < 0) {
            fail(out, "send_packet");
            goto done;
        }
        if (drain_decode(ctx, fr, &planes, &ch, &cap, &got, &rate) < 0) {
            fail(out, "receive_frame");
            goto done;
        }
        pos += (size_t)fl;
        consumed = pos;
    }
    avcodec_send_packet(ctx, NULL);
    drain_decode(ctx, fr, &planes, &ch, &cap, &got, &rate);
    rc = finish_pcm(out, planes, ch, got, rate, consumed, "in_process_au");
done:
    free_planes(planes, ch);
    av_frame_free(&fr);
    av_packet_free(&pkt);
    avcodec_free_context(&ctx);
    av_dict_free(&opts);
    return rc;
}

typedef struct {
    const uint8_t *data;
    size_t len;
    size_t pos;
} MemBuf;

static int mem_read(void *opaque, uint8_t *buf, int buf_size) {
    MemBuf *m = opaque;
    size_t n = (size_t)buf_size;
    if (m->pos >= m->len)
        return AVERROR_EOF;
    if (n > m->len - m->pos)
        n = m->len - m->pos;
    memcpy(buf, m->data + m->pos, n);
    m->pos += n;
    return (int)n;
}

static int64_t mem_seek(void *opaque, int64_t offset, int whence) {
    MemBuf *m = opaque;
    int64_t pos;
    if (whence == AVSEEK_SIZE)
        return (int64_t)m->len;
    if (whence == SEEK_CUR)
        pos = (int64_t)m->pos + offset;
    else if (whence == SEEK_END)
        pos = (int64_t)m->len + offset;
    else
        pos = offset;
    if (pos < 0 || (size_t)pos > m->len)
        return -1;
    m->pos = (size_t)pos;
    return pos;
}

int avc_decode_container(const uint8_t *data, size_t len, AvcPcm *out) {
    AVFormatContext *fmt = NULL;
    AVIOContext *aio = NULL;
    uint8_t *iobuf = NULL;
    AVCodecContext *ctx = NULL;
    const AVCodec *dec;
    AVDictionary *opts = NULL;
    AVPacket *pkt = NULL;
    AVFrame *fr = NULL;
    MemBuf mem = {data, len, 0};
    float **planes = NULL;
    int ch = 0, cap = 0, got = 0, stream = -1, i, rc = -1;
    uint32_t rate = 0;

    memset(out, 0, sizeof(*out));
    snprintf(out->lane, sizeof(out->lane), "in_process_container");
    fmt = avformat_alloc_context();
    iobuf = av_malloc(4096);
    pkt = av_packet_alloc();
    fr = av_frame_alloc();
    if (!fmt || !iobuf || !pkt || !fr) {
        fail(out, "alloc");
        goto done;
    }
    aio = avio_alloc_context(iobuf, 4096, 0, &mem, mem_read, NULL, mem_seek);
    if (!aio) {
        fail(out, "avio");
        goto done;
    }
    iobuf = NULL;
    fmt->pb = aio;
    if (avformat_open_input(&fmt, NULL, NULL, NULL) < 0) {
        fail(out, "avformat_open_input");
        goto done;
    }
    if (avformat_find_stream_info(fmt, NULL) < 0) {
        fail(out, "find_stream_info");
        goto done;
    }
    for (i = 0; i < (int)fmt->nb_streams; i++) {
        if (fmt->streams[i]->codecpar->codec_type == AVMEDIA_TYPE_AUDIO) {
            stream = i;
            break;
        }
    }
    if (stream < 0) {
        fail(out, "no audio stream");
        goto done;
    }
    dec = avcodec_find_decoder(fmt->streams[stream]->codecpar->codec_id);
    if (!dec) {
        fail(out, "no decoder for stream");
        goto done;
    }
    ctx = avcodec_alloc_context3(dec);
    if (!ctx || avcodec_parameters_to_context(ctx, fmt->streams[stream]->codecpar) < 0) {
        fail(out, "codecpar");
        goto done;
    }
    av_dict_set(&opts, "threads", "1", 0);
    if (avcodec_open2(ctx, dec, &opts) < 0) {
        fail(out, "avcodec_open2");
        goto done;
    }
    while (av_read_frame(fmt, pkt) >= 0) {
        if (pkt->stream_index == stream) {
            if (avcodec_send_packet(ctx, pkt) < 0) {
                av_packet_unref(pkt);
                fail(out, "send_packet");
                goto done;
            }
            if (drain_decode(ctx, fr, &planes, &ch, &cap, &got, &rate) < 0) {
                av_packet_unref(pkt);
                fail(out, "receive_frame");
                goto done;
            }
        }
        av_packet_unref(pkt);
    }
    avcodec_send_packet(ctx, NULL);
    drain_decode(ctx, fr, &planes, &ch, &cap, &got, &rate);
    rc = finish_pcm(out, planes, ch, got, rate, len, "in_process_container");
done:
    free_planes(planes, ch);
    av_frame_free(&fr);
    av_packet_free(&pkt);
    avcodec_free_context(&ctx);
    av_dict_free(&opts);
    if (fmt) {
        avformat_close_input(&fmt);
        aio = NULL;
    }
    if (aio) {
        av_freep(&aio->buffer);
        avio_context_free(&aio);
    }
    av_free(iobuf);
    return rc;
}

static int sfi_for_rate(uint32_t rate) {
    static const uint32_t tab[] = {96000, 88200, 64000, 48000, 44100, 32000,
                                   24000, 22050, 16000, 12000, 11025, 8000};
    int i;
    for (i = 0; i < 12; i++)
        if (tab[i] == rate)
            return i;
    return 3;
}

static void write_adts(uint8_t h[7], int sfi, int ch, int framelen) {
    h[0] = 0xff;
    h[1] = 0xf1;
    h[2] = (uint8_t)((1 << 6) | (sfi << 2) | (ch >> 2));
    h[3] = (uint8_t)(((ch & 3) << 6) | (framelen >> 11));
    h[4] = (uint8_t)((framelen >> 3) & 0xff);
    h[5] = (uint8_t)(((framelen & 7) << 5) | 0x1f);
    h[6] = 0xfc;
}

void avc_enc_free(AvcEnc *enc) {
    if (!enc)
        return;
    free(enc->adts);
    enc->adts = NULL;
    enc->adts_len = 0;
}

int avc_encode_lc_adts(const float *const *planes, int channels, int samples,
                       uint32_t rate, uint32_t bitrate_bps, AvcEnc *out) {
    const AVCodec *enc;
    AVCodecContext *ctx = NULL;
    AVDictionary *opts = NULL;
    AVFrame *fr = NULL;
    AVPacket *pkt = NULL;
    uint8_t *buf = NULL;
    size_t cap = 0, len = 0, payload = 0;
    int frame_sz, off = 0, rc = -1, sfi, c, i;

    memset(out, 0, sizeof(*out));
    if (channels < 1 || channels > 2 || samples <= 0)
        return snprintf(out->error, sizeof(out->error), "unsupported layout"), -1;
    enc = avcodec_find_encoder(AV_CODEC_ID_AAC);
    if (!enc)
        return snprintf(out->error, sizeof(out->error), "aac encoder missing"), -1;
    ctx = avcodec_alloc_context3(enc);
    fr = av_frame_alloc();
    pkt = av_packet_alloc();
    if (!ctx || !fr || !pkt) {
        snprintf(out->error, sizeof(out->error), "alloc");
        goto done;
    }
    ctx->sample_rate = (int)rate;
    ctx->bit_rate = (int64_t)bitrate_bps;
    ctx->profile = AV_PROFILE_AAC_LOW;
    ctx->sample_fmt = AV_SAMPLE_FMT_FLTP;
    av_channel_layout_default(&ctx->ch_layout, channels);
    av_dict_set(&opts, "threads", "1", 0);
    if (avcodec_open2(ctx, enc, &opts) < 0) {
        snprintf(out->error, sizeof(out->error), "avcodec_open2 encode");
        goto done;
    }
    frame_sz = ctx->frame_size > 0 ? ctx->frame_size : 1024;
    sfi = sfi_for_rate(rate);
    fr->format = ctx->sample_fmt;
    fr->sample_rate = (int)rate;
    fr->nb_samples = frame_sz;
    if (av_channel_layout_copy(&fr->ch_layout, &ctx->ch_layout) < 0 ||
        av_frame_get_buffer(fr, 0) < 0) {
        char ebuf[64];
        av_strerror(AVERROR(ENOMEM), ebuf, sizeof(ebuf));
        snprintf(out->error, sizeof(out->error),
                 "frame buffer fmt=%d fs=%d ch=%d", ctx->sample_fmt, frame_sz,
                 channels);
        goto done;
    }

    while (off < samples) {
        int n = samples - off;
        if (n > frame_sz)
            n = frame_sz;
        if (av_frame_make_writable(fr) < 0) {
            snprintf(out->error, sizeof(out->error), "frame writable");
            goto done;
        }
        for (c = 0; c < channels; c++) {
            memcpy(fr->extended_data[c], planes[c] + off, (size_t)n * sizeof(float));
            if (n < frame_sz) {
                for (i = n; i < frame_sz; i++)
                    ((float *)fr->extended_data[c])[i] = 0.f;
            }
        }
        if (avcodec_send_frame(ctx, fr) < 0) {
            snprintf(out->error, sizeof(out->error), "send_frame");
            goto done;
        }
        for (;;) {
            int r = avcodec_receive_packet(ctx, pkt);
            uint8_t hdr[7];
            int fl;
            uint8_t *nbuf;
            if (r == AVERROR(EAGAIN) || r == AVERROR_EOF)
                break;
            if (r < 0) {
                snprintf(out->error, sizeof(out->error), "receive_packet");
                goto done;
            }
            fl = 7 + pkt->size;
            write_adts(hdr, sfi, channels, fl);
            if (len + (size_t)fl > cap) {
                cap = cap ? cap * 2 : 4096;
                while (cap < len + (size_t)fl)
                    cap *= 2;
                nbuf = realloc(buf, cap);
                if (!nbuf) {
                    snprintf(out->error, sizeof(out->error), "oom");
                    goto done;
                }
                buf = nbuf;
            }
            memcpy(buf + len, hdr, 7);
            memcpy(buf + len + 7, pkt->data, (size_t)pkt->size);
            len += (size_t)fl;
            payload += (size_t)pkt->size;
            av_packet_unref(pkt);
        }
        off += n;
    }
    avcodec_send_frame(ctx, NULL);
    for (;;) {
        int r = avcodec_receive_packet(ctx, pkt);
        uint8_t hdr[7];
        int fl;
        uint8_t *nbuf;
        if (r == AVERROR_EOF || r == AVERROR(EAGAIN))
            break;
        if (r < 0) {
            snprintf(out->error, sizeof(out->error), "flush packet");
            goto done;
        }
        fl = 7 + pkt->size;
        write_adts(hdr, sfi, channels, fl);
        if (len + (size_t)fl > cap) {
            cap = cap ? cap * 2 : 4096;
            while (cap < len + (size_t)fl)
                cap *= 2;
            nbuf = realloc(buf, cap);
            if (!nbuf) {
                snprintf(out->error, sizeof(out->error), "oom");
                goto done;
            }
            buf = nbuf;
        }
        memcpy(buf + len, hdr, 7);
        memcpy(buf + len + 7, pkt->data, (size_t)pkt->size);
        len += (size_t)fl;
        payload += (size_t)pkt->size;
        av_packet_unref(pkt);
    }
    if (len == 0) {
        snprintf(out->error, sizeof(out->error), "empty encode");
        goto done;
    }
    out->adts = buf;
    buf = NULL;
    out->adts_len = len;
    out->payload_len = payload;
    rc = 0;
done:
    free(buf);
    av_frame_free(&fr);
    av_packet_free(&pkt);
    avcodec_free_context(&ctx);
    av_dict_free(&opts);
    return rc;
}
