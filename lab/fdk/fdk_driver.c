/* Opt-in FDK lab CLI. Never built by cargo test --workspace. */

#include "fdk_adapt.h"

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
    fprintf(stderr, "usage: %s id | decode-adts FILE\n", argv[0]);
    return 2;
}
