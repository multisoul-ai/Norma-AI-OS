#pragma once
#include <stdint.h>

/// Create a private virtual display for this driver process, touching the main display
/// only at its bottom-right corner. Returns its CGDirectDisplayID, or 0 on failure.
/// The display lives until the process exits.
uint32_t NormaE2ECreateVirtualDisplay(uint32_t width, uint32_t height);
