#pragma once

#ifdef __cplusplus
extern "C" {
#endif

/** Start Sparkle background checks if Sparkle.framework is bundled. */
void keel_sparkle_init(void);

/** Show the Sparkle "check for updates" UI. */
void keel_sparkle_check_for_updates(void);

#ifdef __cplusplus
}
#endif
