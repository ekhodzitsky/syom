#define _POSIX_C_SOURCE 200809L
/* In-process fdk-aac 2.0.3 timer. Not built or spawned by cargo test.
 * Times fdk_decode_adts / fdk_encode_pcm_adts only. Process start, file
 * reads, and this process's own printf are outside the timed region.
 * Nothing is subtracted from a process-wall measurement.
 */

#include "fdk_adapt.h"

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

static double now_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (double)ts.tv_sec * 1e3 + (double)ts.tv_nsec / 1e6;
}

static int cmp_double(const void *a, const void *b) {
    double x = *(const double *)a;
    double y = *(const double *)b;
    return (x > y) - (x < y);
}

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
    buf = malloc((size_t)n ? (size_t)n : 1);
    if (!buf || (n > 0 && fread(buf, 1, (size_t)n, f) != (size_t)n)) {
        free(buf);
        fclose(f);
        return NULL;
    }
    fclose(f);
    *len = (size_t)n;
    return buf;
}

static void print_reps(const double *ms, int n) {
    int i;
    printf("[");
    for (i = 0; i < n; i++)
        printf("%s%.4f", i ? "," : "", ms[i]);
    printf("]");
}

static int bench_decode(const char *path, int warm, int reps) {
    size_t n = 0;
    uint8_t *buf = read_all(path, &n);
    double *ms;
    FdkPcm pcm;
    int i, rc;
    double med;
    if (!buf) {
        fprintf(stderr, "read %s\n", path);
        return 2;
    }
    ms = calloc((size_t)reps, sizeof(double));
    if (!ms)
        return 2;
    memset(&pcm, 0, sizeof(pcm));
    for (i = 0; i < warm; i++) {
        rc = fdk_decode_adts(buf, n, &pcm);
        if (rc != 0) {
            fprintf(stderr, "warmup %s\n", pcm.error);
            return 1;
        }
    }
    for (i = 0; i < reps; i++) {
        double t0 = now_ms();
        rc = fdk_decode_adts(buf, n, &pcm);
        ms[i] = now_ms() - t0;
        if (rc != 0) {
            fprintf(stderr, "rep %s\n", pcm.error);
            return 1;
        }
    }
    qsort(ms, (size_t)reps, sizeof(double), cmp_double);
    med = ms[reps / 2];
    printf("{\"engine\":\"%s\",\"op\":\"decode\",\"file\":\"%s\",\"warm\":%d,\"reps\":%d,"
           "\"median_ms\":%.4f,\"min_ms\":%.4f,\"max_ms\":%.4f,"
           "\"rate\":%u,\"channels\":%u,\"samples\":%llu,\"aot\":%d,\"delay\":%d,"
           "\"checksum\":\"0x%016llx\",\"samples_ms\":",
           fdk_engine_id(), path, warm, reps, med, ms[0], ms[reps - 1], pcm.sample_rate,
           pcm.channels, (unsigned long long)pcm.samples, pcm.aot, pcm.delay,
           (unsigned long long)pcm.checksum);
    print_reps(ms, reps);
    printf("}\n");
    free(ms);
    free(buf);
    return 0;
}

static int bench_encode(uint32_t rate, int ch, uint32_t bps, int aot, const char *path, int warm,
                        int reps) {
    size_t raw_n = 0, nsamp;
    uint8_t *raw = read_all(path, &raw_n);
    const float *planes[2];
    double *ms;
    FdkEnc enc;
    int i, c, rc;
    double med;
    if (!raw || (ch != 1 && ch != 2) || raw_n % ((size_t)ch * 4) != 0) {
        fprintf(stderr, "bad pcm\n");
        free(raw);
        return 2;
    }
    nsamp = raw_n / 4 / (size_t)ch;
    for (c = 0; c < ch; c++)
        planes[c] = (const float *)(raw + (size_t)c * nsamp * 4);
    ms = calloc((size_t)reps, sizeof(double));
    if (!ms)
        return 2;
    memset(&enc, 0, sizeof(enc));
    for (i = 0; i < warm + reps; i++) {
        double t0 = now_ms();
        size_t adts_len;
        int delay, after;
        rc = fdk_encode_pcm_adts(planes, ch, (int)nsamp, rate, bps, aot, &enc);
        adts_len = enc.adts_len;
        delay = enc.delay;
        after = enc.afterburner;
        fdk_enc_free(&enc);
        if (i < warm) {
            if (rc != 0) {
                fprintf(stderr, "warmup %s\n", enc.error);
                return 1;
            }
            continue;
        }
        ms[i - warm] = now_ms() - t0;
        if (rc != 0) {
            fprintf(stderr, "rep %s\n", enc.error);
            return 1;
        }
        enc.adts_len = adts_len;
        enc.delay = delay;
        enc.afterburner = after;
    }
    qsort(ms, (size_t)reps, sizeof(double), cmp_double);
    med = ms[reps / 2];
    printf("{\"engine\":\"%s\",\"op\":\"encode\",\"aot\":%d,\"bitrate_bps\":%u,\"warm\":%d,"
           "\"reps\":%d,\"median_ms\":%.4f,\"min_ms\":%.4f,\"max_ms\":%.4f,"
           "\"rate\":%u,\"channels\":%d,\"samples\":%zu,\"adts_bytes\":%zu,\"delay\":%d,"
           "\"afterburner\":%d,\"samples_ms\":",
           fdk_engine_id(), aot, bps, warm, reps, med, ms[0], ms[reps - 1], rate, ch, nsamp,
           enc.adts_len, enc.delay, enc.afterburner);
    print_reps(ms, reps);
    printf("}\n");
    free(ms);
    free(raw);
    return 0;
}

int main(int argc, char **argv) {
    if (argc == 2 && strcmp(argv[1], "id") == 0) {
        printf("%s\n", fdk_engine_id());
        return 0;
    }
    if (argc == 5 && strcmp(argv[1], "bench-decode") == 0)
        return bench_decode(argv[2], atoi(argv[3]), atoi(argv[4]));
    if (argc == 9 && strcmp(argv[1], "bench-encode") == 0)
        return bench_encode((uint32_t)atoi(argv[2]), atoi(argv[3]), (uint32_t)atoi(argv[4]),
                            atoi(argv[5]), argv[6], atoi(argv[7]), atoi(argv[8]));
    fprintf(stderr,
            "usage:\n  %s id\n  %s bench-decode FILE WARM REPS\n"
            "  %s bench-encode RATE CH BPS AOT IN.f32 WARM REPS\n",
            argv[0], argv[0], argv[0]);
    return 2;
}
