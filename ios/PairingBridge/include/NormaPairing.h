#ifndef NORMA_PAIRING_H
#define NORMA_PAIRING_H
#include <stddef.h>
#include <stdint.h>
// Six readable ASCII digits, 33 writable message bytes. NULL means invalid input.
void *norma_pairing_start(const uint8_t *code, uint8_t *message);
// One attempt per handle. 33-byte message, sealed_len ciphertext bytes, 4096-byte output.
// Returns plaintext length or -1. Always free handle afterwards, even on error.
int32_t norma_pairing_finish(void *handle, const uint8_t *message, const uint8_t *sealed,
                            size_t sealed_len, uint8_t *output);
void norma_pairing_free(void *handle);
#endif
