/* Isolated fdk-aac v2.0.3 adapter. Not part of the syom crate. */

#include "fdk_adapt.h"

#include <aacdecoder_lib.h>
#include <aacenc_lib.h>

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

const char *fdk_engine_id(void) { return "fdk-aac-2.0.3 ADTS AOT=2 afterburner=0"; }

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

int fdk_decode_adts(const uint8_t *data, size_t len, FdkPcm *out) {
    HANDLE_AACDECODER dec;
    INT_PCM *frame = NULL;
    float **planes = NULL;
    size_t pos = 0;
    int ch = 0, got = 0, cap = 0, c, i;
    uint32_t rate = 0;
    uint64_t h = 0xcbf29ce484222325ULL;

    memset(out, 0, sizeof(*out));
    snprintf(out->lane, sizeof(out->lane), "in_process_adts");
    dec = aacDecoder_Open(TT_MP4_ADTS, 1);
    if (!dec) {
        snprintf(out->error, sizeof(out->error), "aacDecoder_Open");
        return -1;
    }
    frame = malloc((size_t)8 * 2048 * sizeof(INT_PCM));
    if (!frame) {
        aacDecoder_Close(dec);
        snprintf(out->error, sizeof(out->error), "oom");
        return -1;
    }
    for (;;) {
        AAC_DECODER_ERROR err;
        if (pos < len) {
            UCHAR *ptr = (UCHAR *)(data + pos);
            UINT avail = (UINT)(len - pos);
            UINT valid = avail;
            err = aacDecoder_Fill(dec, &ptr, &avail, &valid);
            if (err != AAC_DEC_OK) {
                snprintf(out->error, sizeof(out->error), "Fill %d", (int)err);
                break;
            }
            pos = len - (size_t)valid;
        }
        err = aacDecoder_DecodeFrame(dec, frame, 8 * 2048, 0);
        if (err == AAC_DEC_NOT_ENOUGH_BITS) {
            if (pos >= len)
                break;
            continue;
        }
        if (err != AAC_DEC_OK) {
            snprintf(out->error, sizeof(out->error), "DecodeFrame %d", (int)err);
            break;
        }
        {
            CStreamInfo *info = aacDecoder_GetStreamInfo(dec);
            int n, nch;
            if (!info || info->sampleRate <= 0 || info->numChannels <= 0)
                continue;
            n = info->frameSize;
            nch = info->numChannels;
            if (ch == 0) {
                ch = nch;
                rate = (uint32_t)info->sampleRate;
                out->aot = info->aot;
                out->delay = info->outputDelay;
                planes = calloc((size_t)ch, sizeof(float *));
                if (!planes)
                    break;
            }
            if (nch != ch)
                break;
            if (got + n > cap) {
                int ncap = cap ? cap * 2 : 8192;
                while (ncap < got + n)
                    ncap *= 2;
                for (c = 0; c < ch; c++) {
                    float *p = realloc(planes[c], (size_t)ncap * sizeof(float));
                    if (!p) {
                        cap = 0;
                        break;
                    }
                    planes[c] = p;
                }
                cap = ncap;
                if (!planes[0])
                    break;
            }
            for (i = 0; i < n; i++)
                for (c = 0; c < ch; c++)
                    planes[c][got + i] = (float)frame[i * ch + c] / 32768.f;
            got += n;
        }
    }
    aacDecoder_Close(dec);
    free(frame);
    if (ch <= 0 || got <= 0) {
        int c2;
        if (planes) {
            for (c2 = 0; c2 < ch; c2++)
                free(planes[c2]);
            free(planes);
        }
        if (!out->error[0])
            snprintf(out->error, sizeof(out->error), "empty pcm");
        return -1;
    }
    out->sample_rate = rate;
    out->channels = (uint32_t)ch;
    out->samples = (uint64_t)got;
    for (c = 0; c < ch; c++)
        h = fnv1a_f32le(h, planes[c], (size_t)got);
    out->checksum = h;
    for (c = 0; c < ch; c++)
        free(planes[c]);
    free(planes);
    return 0;
}

void fdk_enc_free(FdkEnc *enc) {
    if (!enc)
        return;
    free(enc->adts);
    enc->adts = NULL;
}

int fdk_encode_lc_adts(const float *const *planes, int channels, int samples,
                       uint32_t rate, uint32_t bitrate_bps, FdkEnc *out) {
    HANDLE_AACENCODER enc = NULL;
    AACENC_InfoStruct info;
    AACENC_BufDesc in_desc, out_desc;
    AACENC_InArgs in_args;
    AACENC_OutArgs out_args;
    INT_PCM *inbuf = NULL;
    UCHAR *outbuf = NULL;
    void *in_ptr[1], *out_ptr[1];
    INT in_id = IN_AUDIO_DATA, out_id = OUT_BITSTREAM_DATA;
    INT in_el = sizeof(INT_PCM), out_el = sizeof(UCHAR);
    INT in_sz, out_sz = 2048 * 8, in_n, out_n = 1;
    uint8_t *adts = NULL;
    size_t cap = 0, len = 0;
    int off = 0, i, c, frame, rc = -1;

    memset(out, 0, sizeof(*out));
    out->aot = 2;
    out->afterburner = 0;
    out->transmux = 2;
    if (channels < 1 || channels > 2) {
        snprintf(out->error, sizeof(out->error), "unsupported channel mode");
        return -1;
    }
    if (aacEncOpen(&enc, 0, (UINT)channels) != AACENC_OK) {
        snprintf(out->error, sizeof(out->error), "aacEncOpen");
        return -1;
    }
    if (aacEncoder_SetParam(enc, AACENC_AOT, 2) != AACENC_OK ||
        aacEncoder_SetParam(enc, AACENC_SAMPLERATE, rate) != AACENC_OK ||
        aacEncoder_SetParam(enc, AACENC_CHANNELMODE, channels == 1 ? MODE_1 : MODE_2) !=
            AACENC_OK ||
        aacEncoder_SetParam(enc, AACENC_BITRATE, bitrate_bps) != AACENC_OK ||
        aacEncoder_SetParam(enc, AACENC_TRANSMUX, 2) != AACENC_OK ||
        aacEncoder_SetParam(enc, AACENC_AFTERBURNER, 0) != AACENC_OK) {
        snprintf(out->error, sizeof(out->error), "SetParam");
        goto done;
    }
    if (aacEncEncode(enc, NULL, NULL, NULL, NULL) != AACENC_OK ||
        aacEncInfo(enc, &info) != AACENC_OK) {
        snprintf(out->error, sizeof(out->error), "aacEncInfo");
        goto done;
    }
    out->delay = (int)info.nDelay;
    frame = (int)info.frameLength;
    inbuf = malloc((size_t)frame * (size_t)channels * sizeof(INT_PCM));
    outbuf = malloc((size_t)out_sz);
    if (!inbuf || !outbuf) {
        snprintf(out->error, sizeof(out->error), "oom");
        goto done;
    }
    while (off < samples) {
        int n = samples - off;
        if (n > frame)
            n = frame;
        for (i = 0; i < frame; i++) {
            for (c = 0; c < channels; c++) {
                float s = (i < n) ? planes[c][off + i] : 0.f;
                if (s > 1.f)
                    s = 1.f;
                if (s < -1.f)
                    s = -1.f;
                inbuf[i * channels + c] = (INT_PCM)(s * 32767.f);
            }
        }
        in_sz = frame * channels * (INT)sizeof(INT_PCM);
        in_n = 1;
        in_ptr[0] = inbuf;
        in_desc.numBufs = 1;
        in_desc.bufs = in_ptr;
        in_desc.bufferIdentifiers = &in_id;
        in_desc.bufSizes = &in_sz;
        in_desc.bufElSizes = &in_el;
        out_ptr[0] = outbuf;
        out_desc.numBufs = 1;
        out_desc.bufs = out_ptr;
        out_desc.bufferIdentifiers = &out_id;
        out_desc.bufSizes = &out_sz;
        out_desc.bufElSizes = &out_el;
        memset(&in_args, 0, sizeof(in_args));
        in_args.numInSamples = frame * channels;
        memset(&out_args, 0, sizeof(out_args));
        if (aacEncEncode(enc, &in_desc, &out_desc, &in_args, &out_args) != AACENC_OK) {
            snprintf(out->error, sizeof(out->error), "aacEncEncode");
            goto done;
        }
        if (out_args.numOutBytes > 0) {
            if (len + (size_t)out_args.numOutBytes > cap) {
                cap = cap ? cap * 2 : 4096;
                while (cap < len + (size_t)out_args.numOutBytes)
                    cap *= 2;
                {
                    uint8_t *nbuf = realloc(adts, cap);
                    if (!nbuf) {
                        snprintf(out->error, sizeof(out->error), "oom");
                        goto done;
                    }
                    adts = nbuf;
                }
            }
            memcpy(adts + len, outbuf, (size_t)out_args.numOutBytes);
            len += (size_t)out_args.numOutBytes;
        }
        off += n;
    }
    /* flush */
    in_desc.numBufs = 0;
    in_args.numInSamples = -1;
    memset(&out_args, 0, sizeof(out_args));
    aacEncEncode(enc, &in_desc, &out_desc, &in_args, &out_args);
    if (out_args.numOutBytes > 0) {
        uint8_t *nbuf;
        if (len + (size_t)out_args.numOutBytes > cap) {
            cap += (size_t)out_args.numOutBytes + 16;
            nbuf = realloc(adts, cap);
            if (!nbuf) {
                snprintf(out->error, sizeof(out->error), "oom");
                goto done;
            }
            adts = nbuf;
        }
        memcpy(adts + len, outbuf, (size_t)out_args.numOutBytes);
        len += (size_t)out_args.numOutBytes;
    }
    if (len == 0) {
        snprintf(out->error, sizeof(out->error), "empty encode");
        goto done;
    }
    out->adts = adts;
    adts = NULL;
    out->adts_len = len;
    rc = 0;
done:
    free(adts);
    free(inbuf);
    free(outbuf);
    if (enc)
        aacEncClose(&enc);
    return rc;
}
