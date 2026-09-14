/* Isolated FAAC 1.31.1 encoder adapter. Not part of the syom crate. */

#include "faac_adapt.h"

#include <faac.h>

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

const char *faac_engine_id(void) {
    return "faac-1.31.1 ADTS LOW MPEG4 FLOAT TNS=0";
}

void faac_enc_free(FaacEnc *enc) {
    if (!enc)
        return;
    free(enc->adts);
    enc->adts = NULL;
    enc->adts_len = 0;
}

int faac_encode_lc_adts(const float *interleaved, uint32_t channels,
                        uint32_t samples_per_ch, uint32_t rate,
                        uint32_t bitrate_per_ch, FaacEnc *out) {
    faacEncHandle h = NULL;
    faacEncConfigurationPtr cfg;
    unsigned long input_samples = 0, max_out = 0;
    unsigned char *frame = NULL;
    uint8_t *adts = NULL;
    size_t adts_cap = 0, adts_len = 0;
    unsigned long pos = 0;
    int flush = 0;
    char *ver = NULL, *copy = NULL;

    memset(out, 0, sizeof(*out));
    if (!interleaved || channels == 0 || channels > 2 || samples_per_ch == 0 ||
        rate == 0 || bitrate_per_ch == 0) {
        snprintf(out->error, sizeof(out->error), "bad args");
        return -1;
    }
    faacEncGetVersion(&ver, &copy);
    snprintf(out->version, sizeof(out->version), "%s", ver ? ver : "?");

    h = faacEncOpen(rate, channels, &input_samples, &max_out);
    if (!h) {
        snprintf(out->error, sizeof(out->error), "faacEncOpen");
        return -1;
    }
    cfg = faacEncGetCurrentConfiguration(h);
    cfg->mpegVersion = MPEG4;
    cfg->aacObjectType = LOW;
    cfg->jointmode = JOINT_MS;
    cfg->useLfe = 0;
    cfg->useTns = 0;
    cfg->bitRate = bitrate_per_ch;
    cfg->quantqual = 0; /* derived from bitRate */
    cfg->outputFormat = ADTS_STREAM;
    cfg->inputFormat = FAAC_INPUT_FLOAT;
    cfg->shortctl = SHORTCTL_NORMAL;
    if (!faacEncSetConfiguration(h, cfg)) {
        snprintf(out->error, sizeof(out->error), "faacEncSetConfiguration");
        faacEncClose(h);
        return -1;
    }
    cfg = faacEncGetCurrentConfiguration(h);
    out->sample_rate = rate;
    out->channels = channels;
    out->bitrate_per_ch = bitrate_per_ch;
    out->input_samples = input_samples;
    out->max_output_bytes = max_out;
    out->samples_in = samples_per_ch;
    out->mpeg_version = (int)cfg->mpegVersion;
    out->aac_object_type = (int)cfg->aacObjectType;
    out->output_format = (int)cfg->outputFormat;
    out->input_format = (int)cfg->inputFormat;
    out->use_tns = (int)cfg->useTns;
    out->jointmode = (int)cfg->jointmode;
    out->quantqual = cfg->quantqual;

    frame = malloc(max_out);
    if (!frame) {
        snprintf(out->error, sizeof(out->error), "oom");
        faacEncClose(h);
        return -1;
    }

    while (pos < samples_per_ch || flush < 16) {
        unsigned int nch_samples;
        int nout;
        const float *chunk;
        float zeros[4096];
        if (pos < samples_per_ch) {
            unsigned long remain = samples_per_ch - pos;
            unsigned long per_ch = input_samples / channels;
            if (per_ch == 0)
                per_ch = 1024;
            if (remain > per_ch)
                remain = per_ch;
            nch_samples = (unsigned int)(remain * channels);
            chunk = interleaved + (size_t)pos * channels;
            pos += remain;
        } else {
            memset(zeros, 0, sizeof(zeros));
            nch_samples = 0;
            chunk = zeros;
            flush++;
        }
        nout = faacEncEncode(h, (int32_t *)(void *)chunk, nch_samples, frame,
                             (unsigned int)max_out);
        if (nout < 0) {
            snprintf(out->error, sizeof(out->error), "faacEncEncode");
            free(frame);
            free(adts);
            faacEncClose(h);
            return -1;
        }
        if (nout > 0) {
            if (adts_len + (size_t)nout > adts_cap) {
                size_t ncap = adts_cap ? adts_cap * 2 : 8192;
                uint8_t *nbuf;
                while (ncap < adts_len + (size_t)nout)
                    ncap *= 2;
                nbuf = realloc(adts, ncap);
                if (!nbuf) {
                    snprintf(out->error, sizeof(out->error), "oom");
                    free(frame);
                    free(adts);
                    faacEncClose(h);
                    return -1;
                }
                adts = nbuf;
                adts_cap = ncap;
            }
            memcpy(adts + adts_len, frame, (size_t)nout);
            adts_len += (size_t)nout;
        }
        if (pos >= samples_per_ch && nout == 0 && nch_samples == 0)
            break;
    }
    free(frame);
    faacEncClose(h);
    if (adts_len == 0) {
        snprintf(out->error, sizeof(out->error), "no ADTS output");
        free(adts);
        return -1;
    }
    out->adts = adts;
    out->adts_len = adts_len;
    out->flush_calls = flush;
    return 0;
}
