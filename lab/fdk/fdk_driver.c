/* Opt-in FDK lab CLI. Never built by cargo test --workspace. */

#include "fdk_adapt.h"

#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static uint8_t *read_all(const char *path, size_t *len) {
    FILE *f = fopen(path, "rb");
    uint8_t *buf;
    long n;
    if (!f)
        return NULL;
    if (fseek(f, 0, SEEK_END) != 0) {
        fclose(f);
        return NULL;
    }
    n = ftell(f);
    if (n < 0) {
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

int main(int argc, char **argv) {
    if (argc == 2 && strcmp(argv[1], "id") == 0) {
        printf("%s\n", fdk_engine_id());
        return 0;
    }
    if (argc == 3 && strcmp(argv[1], "decode-adts") == 0) {
        size_t n = 0;
        uint8_t *buf = read_all(argv[2], &n);
        FdkPcm pcm;
        int rc;
        if (!buf) {
            fprintf(stderr, "read %s\n", argv[2]);
            return 2;
        }
        rc = fdk_decode_adts(buf, n, &pcm);
        if (rc != 0)
            printf("{\"ok\":false,\"lane\":\"%s\",\"error\":\"%s\"}\n", pcm.lane, pcm.error);
        else
            printf("{\"ok\":true,\"lane\":\"%s\",\"engine\":\"%s\",\"rate\":%u,\"channels\":%u,"
                   "\"samples\":%llu,\"aot\":%d,\"delay\":%d,\"checksum\":\"0x%016llx\"}\n",
                   pcm.lane, fdk_engine_id(), pcm.sample_rate, pcm.channels,
                   (unsigned long long)pcm.samples, pcm.aot, pcm.delay,
                   (unsigned long long)pcm.checksum);
        free(buf);
        return rc == 0 ? 0 : 1;
    }
    if (argc == 6 && strcmp(argv[1], "encode-sine") == 0) {
        uint32_t rate = (uint32_t)atoi(argv[2]);
        int ch = atoi(argv[3]);
        uint32_t bps = (uint32_t)atoi(argv[4]);
        const char *path = argv[5];
        int n, i, c;
        float *planes[2] = {NULL, NULL};
        FdkEnc enc;
        FILE *f;
        if (rate == 0 || (ch != 1 && ch != 2) || bps == 0) {
            fprintf(stderr, "encode-sine RATE CH BITRATE_BPS OUT.adts\n");
            return 2;
        }
        n = (int)(rate * 2u);
        for (c = 0; c < ch; c++) {
            planes[c] = malloc((size_t)n * sizeof(float));
            if (!planes[c])
                return 2;
            for (i = 0; i < n; i++)
                planes[c][i] = (float)(0.5 * sin(2.0 * 3.141592653589793 * 440.0 *
                                                 (double)i / (double)rate));
        }
        if (fdk_encode_lc_adts((const float *const *)planes, ch, n, rate, bps, &enc) !=
            0) {
            printf("{\"ok\":false,\"lane\":\"encode\",\"error\":\"%s\"}\n", enc.error);
            for (c = 0; c < ch; c++)
                free(planes[c]);
            return 1;
        }
        f = fopen(path, "wb");
        if (!f || fwrite(enc.adts, 1, enc.adts_len, f) != enc.adts_len) {
            fprintf(stderr, "write %s\n", path);
            if (f)
                fclose(f);
            fdk_enc_free(&enc);
            for (c = 0; c < ch; c++)
                free(planes[c]);
            return 2;
        }
        fclose(f);
        printf("{\"ok\":true,\"lane\":\"encode\",\"engine\":\"%s\",\"adts_bytes\":%zu,"
               "\"delay\":%d,\"aot\":%d,\"afterburner\":%d,\"bitrate_bps\":%u}\n",
               fdk_engine_id(), enc.adts_len, enc.delay, enc.aot, enc.afterburner, bps);
        fdk_enc_free(&enc);
        for (c = 0; c < ch; c++)
            free(planes[c]);
        return 0;
    }
    fprintf(stderr, "usage: %s id | decode-adts FILE | encode-sine RATE CH BPS OUT\n",
            argv[0]);
    return 2;
}
