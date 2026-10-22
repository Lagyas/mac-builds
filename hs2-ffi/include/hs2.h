/* HS2 – Hypersynth 2: C interface of the Rust engine (hs2-ffi).
 * Keep in sync with src/lib.rs (tests/abi.rs checks the function list).
 *
 * hs2_process runs on the audio thread; all other calls are for the UI /
 * main thread. Strings are UTF-8; the *_name functions copy into buf
 * (NUL-terminated, truncated to cap) and return the full length. */
#ifndef HS2_H
#define HS2_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct HS2Engine HS2Engine;

/* parameter addresses */
#define HS2_PARAM_MASTER 0 /* 0..1 */
#define HS2_PARAM_HYPE   1 /* 0..1 "Output Hype" */
#define HS2_PARAM_TUNE   2 /* -1..1 semitone */

#define HS2_EVENT_MIDI  0
#define HS2_EVENT_PARAM 1

typedef struct HS2Event {
    uint32_t offset;  /* frame inside the buffer; events sorted by offset */
    uint32_t param;   /* HS2_EVENT_PARAM: address */
    float value;      /* HS2_EVENT_PARAM: value */
    uint8_t kind;     /* HS2_EVENT_MIDI / HS2_EVENT_PARAM */
    uint8_t len;      /* HS2_EVENT_MIDI: bytes in data */
    uint8_t data[3];
} HS2Event;

typedef struct HS2Transport {
    double tempo;    /* BPM */
    double beat;     /* song position in quarter notes */
    int32_t playing;
    int32_t valid;   /* 0 = no host transport */
} HS2Transport;

const char *hs2_version(void);

HS2Engine *hs2_engine_new(float sample_rate);
void hs2_engine_free(HS2Engine *e);
void hs2_set_sample_rate(const HS2Engine *e, float sample_rate);

void hs2_process(const HS2Engine *e, float *left, float *right, uint32_t frames,
                 const HS2Event *events, uint32_t n_events, const HS2Transport *transport);
void hs2_midi(const HS2Engine *e, const uint8_t *data, uint32_t len);
void hs2_panic(const HS2Engine *e);

int32_t hs2_load_content_dir(const HS2Engine *e, const char *path);
int32_t hs2_import_content(const char *src, const char *dst);
int32_t hs2_load_soundfont(const HS2Engine *e, const char *path);

uint32_t hs2_bank_count(const HS2Engine *e);
uint32_t hs2_bank_name(const HS2Engine *e, uint32_t bank, char *buf, uint32_t cap);
uint32_t hs2_patch_count(const HS2Engine *e, uint32_t bank);
uint32_t hs2_patch_name(const HS2Engine *e, uint32_t bank, uint32_t patch, char *buf, uint32_t cap);
int32_t hs2_load_patch(const HS2Engine *e, uint32_t part, uint32_t bank, uint32_t patch);
uint32_t hs2_part_patch_name(const HS2Engine *e, uint32_t part, char *buf, uint32_t cap);
int32_t hs2_part_source(const HS2Engine *e, uint32_t part, uint32_t *bank, uint32_t *patch);

uint32_t hs2_combi_count(const HS2Engine *e);
uint32_t hs2_combi_name(const HS2Engine *e, uint32_t idx, char *buf, uint32_t cap);
int32_t hs2_load_combi(const HS2Engine *e, uint32_t idx);

void hs2_set_param(const HS2Engine *e, uint32_t id, float value);
float hs2_get_param(const HS2Engine *e, uint32_t id);
void hs2_meter(const HS2Engine *e, float *left, float *right, uint32_t *voices);
uint32_t hs2_status(const HS2Engine *e, char *buf, uint32_t cap);

uint8_t *hs2_state_save(const HS2Engine *e, uint32_t *len);
void hs2_bytes_free(uint8_t *p, uint32_t len);
int32_t hs2_state_load(const HS2Engine *e, const uint8_t *data, uint32_t len);

#ifdef __cplusplus
}
#endif

#endif
