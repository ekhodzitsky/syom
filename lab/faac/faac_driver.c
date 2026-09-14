/* Opt-in lab CLI. Never built or spawned by cargo test --workspace. */

#include "faac_adapt.h"

#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifndef M_PI
#define M_PI 3.14159265358979323846
#endif

static float *make_sine(uint32_t rate, uint32_t ch, uint32_t samples, double hz) {
    float *pcm = malloc((size_t)samples * ch * sizeof(float));
    uint32_t i, c;
    if (!pcm)
        return NULL;
    for (i = 0; i < samples; i++) {
        float s = (float)(0.5 * sin(2.0 * M_PI * hz * (double)i / (double)rate));
        for (c = 0; c < ch; c++)
            pcm[(size_t)i * ch + c] = s;
    }
    return pcm;
}

static void print_enc(const FaacEnc *e, int rc) {
    if (rc != 0) {
        printf("{\"ok\":false,\"engine\":\"%s\",\"error\":\"%s\"}\n", faac_engine_id(),
               e->error);
        return;
    }
    printf("{\"ok\":true,\"engine\":\"%s\",\"version\":\"%s\",\"rate\":%u,\"channels\":%u,"
           "\"bitrate_per_ch\":%u,\"samples_in\":%lu,\"input_samples\":%lu,"
           "\"max_output_bytes\":%lu,\"adts_bytes\":%zu,\"mpeg_version\":%d,"
           "\"aac_object_type\":%d,\"output_format\":%d,\"input_format\":%d,"
           "\"use_tns\":%d,\"jointmode\":%d,\"quantqual\":%lu,\"flush_calls\":%d}\n",
           faac_engine_id(), e->version, e->sample_rate, e->channels, e->bitrate_per_ch,
           (unsigned long)e->samples_in, e->input_samples, e->max_output_bytes, e->adts_len,
           e->mpeg_version, e->aac_object_type, e->output_format, e->input_format, e->use_tns,
           e->jointmode, e->quantqual, e->flush_calls);
}

int main(int argc, char **argv) {
    if (argc == 2 && strcmp(argv[1], "id") == 0) {
        char *ver = NULL, *copy = NULL;
        printf("%s\n", faac_engine_id());
        (void)copy;
        (void)ver;
        return 0;
    }
    if (argc == 6 && strcmp(argv[1], "encode-sine") == 0) {
        uint32_t rate = (uint32_t)atoi(argv[2]);
        uint32_t ch = (uint32_t)atoi(argv[3]);
        uint32_t bps = (uint32_t)atoi(argv[4]);
        const char *out_path = argv[5];
        uint32_t samples = rate * 2; /* 2 seconds */
        float *pcm;
        FaacEnc enc;
        int rc;
        FILE *f;
        pcm = make_sine(rate, ch, samples, 440.0);
        if (!pcm) {
            fprintf(stderr, "oom\n");
            return 2;
        }
        rc = faac_encode_lc_adts(pcm, ch, samples, rate, bps, &enc);
        print_enc(&enc, rc);
        if (rc == 0) {
            f = fopen(out_path, "wb");
            if (!f || fwrite(enc.adts, 1, enc.adts_len, f) != enc.adts_len) {
                fprintf(stderr, "write %s\n", out_path);
                rc = 2;
            }
            if (f)
                fclose(f);
        }
        free(pcm);
        faac_enc_free(&enc);
        return rc == 0 ? 0 : 1;
    }
    fprintf(stderr,
            "usage:\n  %s id\n  %s encode-sine RATE CHANNELS BITRATE_PER_CH OUT.adts\n",
            argv[0], argv[0]);
    return 2;
}
