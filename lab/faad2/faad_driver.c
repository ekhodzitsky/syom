/* Opt-in lab CLI. Never built or spawned by cargo test --workspace. */

#include "faad_adapt.h"

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

static void print_pcm(const FaadPcm *p, int rc) {
    int i;
    if (rc != 0) {
        printf("{\"ok\":false,\"lane\":\"%s\",\"error\":\"%s\"}\n", p->lane, p->error);
        return;
    }
    printf("{\"ok\":true,\"lane\":\"%s\",\"engine\":\"%s\",\"rate\":%u,\"channels\":%u,"
           "\"samples\":%llu,\"consumed\":%zu,\"checksum\":\"0x%016llx\","
           "\"object_type\":%d,\"sbr\":%d,\"ps\":%d,\"output_format\":%d,"
           "\"channel_position\":[",
           p->lane, faad_engine_id(), p->sample_rate, p->channels,
           (unsigned long long)p->samples, p->consumed, (unsigned long long)p->checksum,
           p->object_type, p->sbr, p->ps, p->output_format);
    for (i = 0; i < (int)p->channels && i < 8; i++) {
        if (i)
            putchar(',');
        printf("%u", (unsigned)p->channel_position[i]);
    }
    printf("]}\n");
}

int main(int argc, char **argv) {
    if (argc == 2 && strcmp(argv[1], "id") == 0) {
        printf("%s\n", faad_engine_id());
        return 0;
    }
    if (argc == 3 && strcmp(argv[1], "decode-adts") == 0) {
        size_t n = 0;
        uint8_t *buf = read_all(argv[2], &n);
        FaadPcm pcm;
        int rc;
        if (!buf) {
            fprintf(stderr, "read %s\n", argv[2]);
            return 2;
        }
        rc = faad_decode_adts(buf, n, &pcm);
        print_pcm(&pcm, rc);
        free(buf);
        return rc == 0 ? 0 : 1;
    }
    fprintf(stderr, "usage:\n  %s id\n  %s decode-adts FILE\n", argv[0], argv[0]);
    return 2;
}
