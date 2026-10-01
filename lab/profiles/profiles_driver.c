/* Isolated FDK profile-expansion driver (TASK-97). Not part of the syom crate.
 * Never built by cargo test --workspace. Links pinned libfdk-aac v2.0.3.
 *
 *   profiles_driver id
 *   profiles_driver decode <adts|loas> IN OUT.s16le
 *   profiles_driver encode IN.s16le RATE CH BPS AOT <adts|loas|raw> OUT [sbr_mode]
 */

#include <aacdecoder_lib.h>
#include <aacenc_lib.h>

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static uint8_t *read_all(const char *path, size_t *len) {
    FILE *f = fopen(path, "rb");
    uint8_t *buf;
    long n;
    if (!f)
        return NULL;
    if (fseek(f, 0, SEEK_END) != 0 || (n = ftell(f)) < 0) {
        fclose(f);
        return NULL;
    }
    rewind(f);
    buf = malloc((size_t)n);
    if (!buf || fread(buf, 1, (size_t)n, f) != (size_t)n) {
        free(buf);
        fclose(f);
        return NULL;
    }
    fclose(f);
    *len = (size_t)n;
    return buf;
}

static int write_all(const char *path, const uint8_t *buf, size_t len) {
    FILE *f = fopen(path, "wb");
    int ok;
    if (!f)
        return -1;
    ok = fwrite(buf, 1, len, f) == len;
    fclose(f);
    return ok ? 0 : -1;
}

static int tt_of(const char *s) {
    if (strcmp(s, "adts") == 0)
        return TT_MP4_ADTS;
    if (strcmp(s, "loas") == 0)
        return TT_MP4_LOAS;
    if (strcmp(s, "raw") == 0)
        return TT_MP4_RAW;
    return -1;
}

static int do_decode(const char *in_path, TRANSPORT_TYPE tt, const char *out_path) {
    size_t len = 0, pos = 0;
    uint8_t *data = read_all(in_path, &len);
    HANDLE_AACDECODER dec;
    INT_PCM *frame = NULL;
    int16_t *pcm = NULL;
    size_t got = 0, cap = 0;
    int ch = 0, rate = 0, aot = 0, delay = 0, frames = 0;
    AAC_DECODER_ERROR err = AAC_DEC_OK;
    uint64_t h = 0xcbf29ce484222325ULL;

    if (!data) {
        printf("{\"ok\":false,\"lane\":\"decode\",\"error\":\"read %s\"}\n", in_path);
        return 2;
    }
    dec = aacDecoder_Open(tt, 1);
    if (!dec) {
        printf("{\"ok\":false,\"lane\":\"decode\",\"error\":\"aacDecoder_Open\"}\n");
        free(data);
        return 1;
    }
    frame = malloc((size_t)8 * 4096 * sizeof(INT_PCM));
    if (!frame)
        return 2;
    for (;;) {
        if (pos < len) {
            UCHAR *ptr = data + pos;
            UINT avail = (UINT)(len - pos), valid = avail;
            err = aacDecoder_Fill(dec, &ptr, &avail, &valid);
            if (err != AAC_DEC_OK)
                break;
            pos = len - (size_t)valid;
        }
        err = aacDecoder_DecodeFrame(dec, frame, 8 * 4096, 0);
        if (err == AAC_DEC_NOT_ENOUGH_BITS) {
            if (pos >= len)
                break;
            continue;
        }
        if (err != AAC_DEC_OK)
            break;
        {
            CStreamInfo *info = aacDecoder_GetStreamInfo(dec);
            int n, nch, i, c;
            if (!info || info->sampleRate <= 0 || info->numChannels <= 0)
                continue;
            n = info->frameSize;
            nch = info->numChannels;
            if (ch == 0) {
                ch = nch;
                rate = info->sampleRate;
                aot = info->aot;
                delay = info->outputDelay;
            }
            if (nch != ch)
                break;
            if (got + (size_t)n * ch > cap) {
                size_t ncap = cap ? cap * 2 : 65536;
                int16_t *np;
                while (ncap < got + (size_t)n * ch)
                    ncap *= 2;
                np = realloc(pcm, ncap * sizeof(int16_t));
                if (!np)
                    break;
                pcm = np;
                cap = ncap;
            }
            for (i = 0; i < n * ch; i++) {
                INT_PCM v = frame[i];
                pcm[got + i] = (int16_t)(v > 32767 ? 32767 : (v < -32768 ? -32768 : v));
            }
            /* FNV-1a over the s16 samples. */
            for (i = 0; i < n * ch; i++) {
                uint16_t b = (uint16_t)pcm[got + i];
                for (c = 0; c < 2; c++) {
                    h ^= (uint64_t)((b >> (8 * c)) & 0xffu);
                    h *= 0x100000001b3ULL;
                }
            }
            got += (size_t)(n * ch);
            frames++;
        }
    }
    aacDecoder_Close(dec);
    free(frame);
    free(data);
    if (err == AAC_DEC_OK || err == AAC_DEC_NOT_ENOUGH_BITS || err == AAC_DEC_TRANSPORT_SYNC_ERROR) {
        if (got == 0) {
            printf("{\"ok\":false,\"lane\":\"decode\",\"error\":\"empty pcm (%d)\"}\n", (int)err);
            free(pcm);
            return 1;
        }
        if (write_all(out_path, (const uint8_t *)pcm, got * sizeof(int16_t)) != 0) {
            printf("{\"ok\":false,\"lane\":\"decode\",\"error\":\"write %s\"}\n", out_path);
            free(pcm);
            return 2;
        }
        printf("{\"ok\":true,\"lane\":\"decode\",\"rate\":%d,\"channels\":%d,"
               "\"samples_per_ch\":%zu,\"frames\":%d,\"aot\":%d,\"output_delay\":%d,"
               "\"checksum\":\"0x%016llx\"}\n",
               rate, ch, ch ? got / (size_t)ch : 0, frames, aot, delay,
               (unsigned long long)h);
        free(pcm);
        return 0;
    }
    printf("{\"ok\":false,\"lane\":\"decode\",\"error\":\"aac err %d\",\"samples_per_ch\":%zu}\n",
           (int)err, ch ? got / (size_t)ch : 0);
    free(pcm);
    return 1;
}

static int do_encode(const char *in_path, uint32_t rate, int ch, uint32_t bps, int aot,
                     TRANSPORT_TYPE tt, const char *out_path, int sbr_mode) {
    size_t pcm_len = 0;
    uint8_t *pcm_raw = read_all(in_path, &pcm_len);
    const int16_t *pcm;
    long total, off = 0;
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
    INT in_sz, out_sz = 8192;
    uint8_t *stream = NULL;
    size_t cap = 0, slen = 0;
    int frames = 0, rc = 1;
    char err_msg[256] = "";

    if (!pcm_raw || ch < 1 || ch > 2) {
        printf("{\"ok\":false,\"lane\":\"encode\",\"error\":\"args/input\"}\n");
        free(pcm_raw);
        return 2;
    }
    pcm = (const int16_t *)pcm_raw;
    total = (long)(pcm_len / sizeof(int16_t) / (size_t)ch);
    if (aacEncOpen(&enc, 0, (UINT)ch) != AACENC_OK) {
        snprintf(err_msg, sizeof(err_msg), "aacEncOpen");
        goto done;
    }
    if (aacEncoder_SetParam(enc, AACENC_AOT, (UINT)aot) != AACENC_OK ||
        aacEncoder_SetParam(enc, AACENC_SAMPLERATE, rate) != AACENC_OK ||
        aacEncoder_SetParam(enc, AACENC_CHANNELMODE, ch == 1 ? MODE_1 : MODE_2) != AACENC_OK ||
        aacEncoder_SetParam(enc, AACENC_BITRATE, bps) != AACENC_OK ||
        aacEncoder_SetParam(enc, AACENC_TRANSMUX, (UINT)tt) != AACENC_OK ||
        aacEncoder_SetParam(enc, AACENC_AFTERBURNER, 0) != AACENC_OK) {
        snprintf(err_msg, sizeof(err_msg), "SetParam basic");
        goto done;
    }
    if (sbr_mode >= 0 &&
        aacEncoder_SetParam(enc, AACENC_SBR_MODE, (UINT)sbr_mode) != AACENC_OK) {
        snprintf(err_msg, sizeof(err_msg), "SetParam sbr");
        goto done;
    }
    if (aacEncEncode(enc, NULL, NULL, NULL, NULL) != AACENC_OK ||
        aacEncInfo(enc, &info) != AACENC_OK) {
        snprintf(err_msg, sizeof(err_msg), "init rejected (unsupported config)");
        goto done;
    }
    inbuf = malloc((size_t)info.frameLength * (size_t)ch * sizeof(INT_PCM));
    outbuf = malloc((size_t)out_sz);
    if (!inbuf || !outbuf) {
        snprintf(err_msg, sizeof(err_msg), "oom");
        goto done;
    }
    while (off < total) {
        long n = total - off, i;
        if (n > (long)info.frameLength)
            n = (long)info.frameLength;
        memset(inbuf, 0, (size_t)info.frameLength * (size_t)ch * sizeof(INT_PCM));
        for (i = 0; i < n * ch; i++)
            inbuf[i] = pcm[off * ch + i];
        in_sz = (INT)(info.frameLength * ch) * (INT)sizeof(INT_PCM);
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
        in_args.numInSamples = (INT)(info.frameLength * ch);
        memset(&out_args, 0, sizeof(out_args));
        if (aacEncEncode(enc, &in_desc, &out_desc, &in_args, &out_args) != AACENC_OK) {
            snprintf(err_msg, sizeof(err_msg), "aacEncEncode frame %d", frames);
            goto done;
        }
        if (out_args.numOutBytes > 0) {
            if (slen + (size_t)out_args.numOutBytes > cap) {
                uint8_t *nb;
                cap = cap ? cap * 2 : 8192;
                while (cap < slen + (size_t)out_args.numOutBytes)
                    cap *= 2;
                nb = realloc(stream, cap);
                if (!nb) {
                    snprintf(err_msg, sizeof(err_msg), "oom");
                    goto done;
                }
                stream = nb;
            }
            memcpy(stream + slen, outbuf, (size_t)out_args.numOutBytes);
            slen += (size_t)out_args.numOutBytes;
        }
        frames++;
        off += n;
    }
    in_desc.numBufs = 0;
    in_args.numInSamples = -1;
    memset(&out_args, 0, sizeof(out_args));
    if (aacEncEncode(enc, &in_desc, &out_desc, &in_args, &out_args) == AACENC_OK &&
        out_args.numOutBytes > 0) {
        uint8_t *nb = realloc(stream, slen + (size_t)out_args.numOutBytes + 16);
        if (nb) {
            stream = nb;
            memcpy(stream + slen, outbuf, (size_t)out_args.numOutBytes);
            slen += (size_t)out_args.numOutBytes;
        }
    }
    if (slen == 0) {
        snprintf(err_msg, sizeof(err_msg), "empty encode");
        goto done;
    }
    if (write_all(out_path, stream, slen) != 0) {
        snprintf(err_msg, sizeof(err_msg), "write %s", out_path);
        goto done;
    }
    printf("{\"ok\":true,\"lane\":\"encode\",\"rate\":%u,\"channels\":%d,"
           "\"bitrate_bps\":%u,\"aot\":%d,\"tt\":%d,\"sbr_mode\":%d,"
           "\"frames\":%d,\"frame_length\":%u,\"enc_delay\":%d,"
           "\"stream_bytes\":%zu,\"actual_bps\":%.1f,\"asc_hex\":\"",
           rate, ch, bps, aot, (int)tt, sbr_mode, frames, info.frameLength,
           (int)info.nDelay, slen,
           total > 0 ? (double)slen * 8.0 * (double)rate / (double)total : 0.0);
    {
        UINT i;
        for (i = 0; i < info.confSize && i < 64; i++)
            printf("%02x", info.confBuf[i]);
    }
    printf("\"}\n");
    rc = 0;
done:
    if (rc != 0)
        printf("{\"ok\":false,\"lane\":\"encode\",\"aot\":%d,\"error\":\"%s\"}\n", aot,
               err_msg[0] ? err_msg : "unknown");
    free(stream);
    free(inbuf);
    free(outbuf);
    free(pcm_raw);
    if (enc)
        aacEncClose(&enc);
    return rc;
}

int main(int argc, char **argv) {
    if (argc == 2 && strcmp(argv[1], "id") == 0) {
        printf("fdk-aac-2.0.3 profiles driver (TASK-97)\n");
        return 0;
    }
    if (argc == 5 && strcmp(argv[1], "decode") == 0) {
        int tt = tt_of(argv[2]);
        if (tt < 0 || tt == TT_MP4_RAW) {
            fprintf(stderr, "decode <adts|loas> IN OUT.s16le\n");
            return 2;
        }
        return do_decode(argv[3], (TRANSPORT_TYPE)tt, argv[4]);
    }
    if ((argc == 9 || argc == 10) && strcmp(argv[1], "encode") == 0) {
        int tt = tt_of(argv[7]);
        int sbr = argc == 10 ? atoi(argv[9]) : -1;
        if (tt < 0) {
            fprintf(stderr, "encode IN.s16le RATE CH BPS AOT <adts|loas|raw> OUT [sbr_mode]\n");
            return 2;
        }
        return do_encode(argv[2], (uint32_t)atoi(argv[3]), atoi(argv[4]),
                         (uint32_t)atoi(argv[5]), atoi(argv[6]),
                         (TRANSPORT_TYPE)tt, argv[8], sbr);
    }
    fprintf(stderr,
            "usage: %s id | decode <adts|loas> IN OUT.s16le | "
            "encode IN.s16le RATE CH BPS AOT <adts|loas|raw> OUT [sbr_mode]\n",
            argv[0]);
    return 2;
}
