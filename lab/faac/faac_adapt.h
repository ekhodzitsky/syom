#ifndef SYOM_LAB_FAAC_ADAPT_H
#define SYOM_LAB_FAAC_ADAPT_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    uint8_t *adts;
    size_t adts_len;
    uint32_t sample_rate;
    uint32_t channels;
    uint32_t bitrate_per_ch;
    unsigned long input_samples;     /* from faacEncOpen, all channels */
    unsigned long max_output_bytes;
    unsigned long samples_in;        /* PCM frames (per channel) fed */
    int mpeg_version;
    int aac_object_type;
    int output_format;
    int input_format;
    int use_tns;
    int jointmode;
    unsigned long quantqual;
    int flush_calls;
    char version[64];
    char error[256];
} FaacEnc;

int faac_encode_lc_adts(const float *interleaved, uint32_t channels,
                        uint32_t samples_per_ch, uint32_t rate,
                        uint32_t bitrate_per_ch, FaacEnc *out);
void faac_enc_free(FaacEnc *enc);
const char *faac_engine_id(void);

#ifdef __cplusplus
}
#endif
#endif
