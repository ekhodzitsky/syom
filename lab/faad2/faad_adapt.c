/* Isolated FAAD2 2.11.3 decode adapter. Not part of the syom crate. */

#include "faad_adapt.h"

#include <neaacdec.h>

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

const char *faad_engine_id(void) { return "faad2-2.11.3 FAAD_FMT_FLOAT ADTS"; }

static uint64_t fnv1a_f32le(uint64_t h, const float *p, size_t n) {
    const uint64_t prime = 0x100000001b3ULL;
    size_t i;
    int b;
    for (i = 0; i < n; i++) {
        uint32_t bits;
        memcpy(&bits, &p[i], 4);
        for (b = 0; b < 4; b++) {
            h ^= (uint64_t)((bits >> (8 * b)) & 0xffu);
            h *= prime;
        }
    }
    return h;
}

int faad_decode_adts(const uint8_t *data, size_t len, FaadPcm *out) {
    NeAACDecHandle h;
    NeAACDecConfigurationPtr cfg;
    unsigned long sr = 0;
    unsigned char nch = 0;
    long init;
    size_t pos;
    float **planes = NULL;
    int ch = 0, got = 0, cap = 0, c, i;
    uint32_t rate = 0;

    memset(out, 0, sizeof(*out));
    snprintf(out->lane, sizeof(out->lane), "in_process_adts");
    out->output_format = FAAD_FMT_FLOAT;
    if (!data || len == 0) {
        snprintf(out->error, sizeof(out->error), "empty input");
        return -1;
    }
    h = NeAACDecOpen();
    if (!h) {
        snprintf(out->error, sizeof(out->error), "NeAACDecOpen");
        return -1;
    }
    cfg = NeAACDecGetCurrentConfiguration(h);
    cfg->outputFormat = FAAD_FMT_FLOAT;
    cfg->downMatrix = 0;
    cfg->dontUpSampleImplicitSBR = 0;
    if (!NeAACDecSetConfiguration(h, cfg)) {
        NeAACDecClose(h);
        snprintf(out->error, sizeof(out->error), "SetConfiguration");
        return -1;
    }
    init = NeAACDecInit(h, (unsigned char *)data, (unsigned long)len, &sr, &nch);
    if (init < 0) {
        snprintf(out->error, sizeof(out->error), "Init %s", NeAACDecGetErrorMessage((unsigned char)(-init)));
        NeAACDecClose(h);
        return -1;
    }
    pos = (size_t)init;
    while (pos < len) {
        NeAACDecFrameInfo info;
        void *pcm;
        unsigned long ntot, n;
        memset(&info, 0, sizeof(info));
        pcm = NeAACDecDecode(h, &info, (unsigned char *)data + pos, (unsigned long)(len - pos));
        if (info.error) {
            snprintf(out->error, sizeof(out->error), "Decode %s", NeAACDecGetErrorMessage(info.error));
            break;
        }
        if (info.bytesconsumed == 0)
            break;
        pos += info.bytesconsumed;
        if (!pcm || info.samples == 0 || info.channels == 0)
            continue;
        ntot = info.samples;
        n = ntot / info.channels;
        if (ch == 0) {
            ch = (int)info.channels;
            rate = (uint32_t)info.samplerate;
            out->object_type = (int)info.object_type;
            out->sbr = (int)info.sbr;
            out->ps = (int)info.ps;
            memcpy(out->channel_position, info.channel_position,
                   (size_t)(ch < 8 ? ch : 8));
            planes = calloc((size_t)ch, sizeof(float *));
            if (!planes) {
                snprintf(out->error, sizeof(out->error), "oom");
                break;
            }
        }
        if ((int)info.channels != ch || (uint32_t)info.samplerate != rate) {
            snprintf(out->error, sizeof(out->error), "layout change");
            break;
        }
        if (got + (int)n > cap) {
            int ncap = cap == 0 ? 8192 : cap;
            while (ncap < got + (int)n)
                ncap *= 2;
            for (c = 0; c < ch; c++) {
                float *p = realloc(planes[c], (size_t)ncap * sizeof(float));
                if (!p) {
                    snprintf(out->error, sizeof(out->error), "oom");
                    goto done;
                }
                planes[c] = p;
            }
            cap = ncap;
        }
        {
            const float *inter = (const float *)pcm;
            for (i = 0; i < (int)n; i++)
                for (c = 0; c < ch; c++)
                    planes[c][got + i] = inter[i * ch + c];
        }
        got += (int)n;
    }
done:
    if (planes && ch > 0 && got > 0 && out->error[0] == 0) {
        uint64_t hsum = 0xcbf29ce484222325ULL;
        out->sample_rate = rate;
        out->channels = (uint32_t)ch;
        out->samples = (uint64_t)got;
        out->consumed = pos;
        for (c = 0; c < ch; c++)
            hsum = fnv1a_f32le(hsum, planes[c], (size_t)got);
        out->checksum = hsum;
        for (c = 0; c < ch; c++)
            free(planes[c]);
        free(planes);
        NeAACDecClose(h);
        return 0;
    }
    if (planes) {
        for (c = 0; c < ch; c++)
            free(planes[c]);
        free(planes);
    }
    NeAACDecClose(h);
    if (out->error[0] == 0)
        snprintf(out->error, sizeof(out->error), "empty pcm");
    return -1;
}
