#ifndef SYOM_LAB_FDK_ADAPT_H
#define SYOM_LAB_FDK_ADAPT_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    uint32_t sample_rate;
    uint32_t channels;
    uint64_t samples;
    uint64_t checksum;
    int aot;
    int delay;
    char lane[32];
    char error[256];
} FdkPcm;

typedef struct {
    uint8_t *adts;
    size_t adts_len;
    int delay;
    int afterburner;
    int aot;
    int transmux;
    char error[256];
} FdkEnc;

int fdk_decode_adts(const uint8_t *data, size_t len, FdkPcm *out);
int fdk_encode_lc_adts(const float *const *planes, int channels, int samples,
                       uint32_t rate, uint32_t bitrate_bps, FdkEnc *out);
/* aot: 2 = AAC-LC, 5 = HE-AAC v1 (SBR), 29 = HE-AAC v2 (SBR+PS, stereo). */
int fdk_encode_pcm_adts(const float *const *planes, int channels, int samples,
                        uint32_t rate, uint32_t bitrate_bps, int aot,
                        FdkEnc *out);
void fdk_enc_free(FdkEnc *enc);
const char *fdk_engine_id(void);

#ifdef __cplusplus
}
#endif
#endif
