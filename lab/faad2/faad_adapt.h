#ifndef SYOM_LAB_FAAD_ADAPT_H
#define SYOM_LAB_FAAD_ADAPT_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    uint32_t sample_rate;
    uint32_t channels;
    uint64_t samples; /* per channel */
    uint64_t checksum;
    size_t consumed;
    int object_type;
    int sbr; /* NeAACDecFrameInfo.sbr */
    int ps;
    int output_format; /* FAAD_FMT_FLOAT = 4 */
    unsigned char channel_position[8];
    char lane[32];
    char error[256];
} FaadPcm;

int faad_decode_adts(const uint8_t *data, size_t len, FaadPcm *out);
const char *faad_engine_id(void);

#ifdef __cplusplus
}
#endif
#endif
