/* Opt-in lab CLI. Never built or spawned by cargo test --workspace. */

#include "avc_adapt.h"

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
    if (!buf) {
        fclose(f);
        return NULL;
    }
    if (fread(buf, 1, (size_t)n, f) != (size_t)n) {
        free(buf);
        fclose(f);
        return NULL;
    }
    fclose(f);
    *len = (size_t)n;
    return buf;
}

static void print_pcm(const AvcPcm *p, int rc) {
    if (rc != 0) {
        printf("{\"ok\":false,\"lane\":\"%s\",\"error\":\"%s\"}\n", p->lane, p->error);
        return;
    }
    printf("{\"ok\":true,\"lane\":\"%s\",\"engine\":\"%s\",\"rate\":%u,\"channels\":%u,"
           "\"samples\":%llu,\"consumed\":%zu,\"checksum\":\"0x%016llx\"}\n",
           p->lane, avc_engine_id(), p->sample_rate, p->channels,
           (unsigned long long)p->samples, p->consumed,
           (unsigned long long)p->checksum);
}

int main(int argc, char **argv) {
    if (argc == 2 && strcmp(argv[1], "id") == 0) {
        printf("%s\n", avc_engine_id());
        return 0;
    }
    if (argc == 3 &&
        (strcmp(argv[1], "decode-au") == 0 || strcmp(argv[1], "decode-container") == 0)) {
        size_t n = 0;
        uint8_t *buf = read_all(argv[2], &n);
        AvcPcm pcm;
        int rc;
        if (!buf) {
            fprintf(stderr, "read %s\n", argv[2]);
            return 2;
        }
        rc = strcmp(argv[1], "decode-au") == 0 ? avc_decode_au(buf, n, &pcm)
                                               : avc_decode_container(buf, n, &pcm);
        print_pcm(&pcm, rc);
        free(buf);
        return rc == 0 ? 0 : 1;
    }
    if (argc == 7 && strcmp(argv[1], "encode-lc-adts") == 0) {
        uint32_t rate = (uint32_t)atoi(argv[2]);
        int ch = atoi(argv[3]);
        uint32_t bps = (uint32_t)atoi(argv[4]);
        size_t nbytes = 0, samples;
        uint8_t *raw = read_all(argv[5], &nbytes);
        float *owned = NULL;
        const float *planes[2];
        AvcEnc enc;
        int c, rc;
        FILE *out;
        if (!raw || ch < 1 || ch > 2 || rate == 0) {
            fprintf(stderr, "bad encode args\n");
            free(raw);
            return 2;
        }
        samples = nbytes / (sizeof(float) * (size_t)ch);
        owned = (float *)raw;
        for (c = 0; c < ch; c++)
            planes[c] = owned + (size_t)c * samples;
        rc = avc_encode_lc_adts(planes, ch, (int)samples, rate, bps, &enc);
        if (rc != 0) {
            printf("{\"ok\":false,\"lane\":\"in_process_encode_adts\",\"error\":\"%s\"}\n",
                   enc.error);
            free(raw);
            return 1;
        }
        out = fopen(argv[6], "wb");
        if (!out || fwrite(enc.adts, 1, enc.adts_len, out) != enc.adts_len) {
            fprintf(stderr, "write %s\n", argv[6]);
            avc_enc_free(&enc);
            free(raw);
            if (out)
                fclose(out);
            return 2;
        }
        fclose(out);
        printf("{\"ok\":true,\"lane\":\"in_process_encode_adts\",\"engine\":\"%s\","
               "\"adts_bytes\":%zu,\"payload_bytes\":%zu}\n",
               avc_engine_id(), enc.adts_len, enc.payload_len);
        avc_enc_free(&enc);
        free(raw);
        return 0;
    }
    fprintf(stderr,
            "usage:\n  %s id\n  %s decode-au FILE\n  %s decode-container FILE\n"
            "  %s encode-lc-adts RATE CH BITRATE IN.f32le.planar OUT.adts\n",
            argv[0], argv[0], argv[0], argv[0]);
    return 2;
}
