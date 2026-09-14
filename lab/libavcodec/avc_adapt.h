#ifndef SYOM_LAB_AVC_ADAPT_H
#define SYOM_LAB_AVC_ADAPT_H

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
    size_t consumed; /* input bytes consumed */
    char lane[32];
    char error[256];
} AvcPcm;

typedef struct {
    uint8_t *adts;
    size_t adts_len;
    size_t payload_len;
    char error[256];
} AvcEnc;

/* libavcodec only: walk ADTS frames as access units. */
int avc_decode_au(const uint8_t *data, size_t len, AvcPcm *out);

/* libavformat + libavcodec (demux included; not an AU timing cell). */
int avc_decode_container(const uint8_t *data, size_t len, AvcPcm *out);

/* LC encode, planar f32 native rate, ADTS out. */
int avc_encode_lc_adts(const float *const *planes, int channels, int samples,
                       uint32_t rate, uint32_t bitrate_bps, AvcEnc *out);

void avc_enc_free(AvcEnc *enc);

const char *avc_engine_id(void);

#ifdef __cplusplus
}
#endif
#endif
