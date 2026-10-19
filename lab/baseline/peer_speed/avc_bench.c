#define _POSIX_C_SOURCE 200809L
/* In-process FFmpeg 9.0.1 native AAC timer. Not built by cargo test.
 * Times avc_decode_au only (libavcodec ADTS access units, threads=1).
 * Process start and the file read are outside the timed region.
 */

#include "avc_adapt.h"

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

int main(int argc, char **argv) {
    size_t n = 0;
    uint8_t *buf;
    double *ms;
    AvcPcm pcm;
    int warm, reps, i, rc;
    double med;
    if (argc == 2 && strcmp(argv[1], "id") == 0) {
        printf("%s\n", avc_engine_id());
        return 0;
    }
    if (argc != 5 || strcmp(argv[1], "bench-decode") != 0) {
        fprintf(stderr, "usage: %s id | bench-decode FILE WARM REPS\n", argv[0]);
        return 2;
    }
    warm = atoi(argv[3]);
    reps = atoi(argv[4]);
    buf = read_all(argv[2], &n);
    if (!buf || reps < 1) {
        fprintf(stderr, "read %s\n", argv[2]);
        return 2;
    }
    ms = calloc((size_t)reps, sizeof(double));
    if (!ms)
        return 2;
    memset(&pcm, 0, sizeof(pcm));
    for (i = 0; i < warm; i++) {
        rc = avc_decode_au(buf, n, &pcm);
        if (rc != 0) {
            fprintf(stderr, "warmup %s\n", pcm.error);
            return 1;
        }
    }
    for (i = 0; i < reps; i++) {
        double t0 = now_ms();
        rc = avc_decode_au(buf, n, &pcm);
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
           "\"rate\":%u,\"channels\":%u,\"samples\":%llu,\"consumed\":%zu,"
           "\"checksum\":\"0x%016llx\",\"samples_ms\":[",
           avc_engine_id(), argv[2], warm, reps, med, ms[0], ms[reps - 1], pcm.sample_rate,
           pcm.channels, (unsigned long long)pcm.samples, pcm.consumed,
           (unsigned long long)pcm.checksum);
    for (i = 0; i < reps; i++)
        printf("%s%.4f", i ? "," : "", ms[i]);
    printf("]}\n");
    free(ms);
    free(buf);
    return 0;
}
