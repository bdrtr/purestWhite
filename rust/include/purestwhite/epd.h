// C ABI of rust/crates/bembeyaz-ffi/src/epd.rs (rust/crates/purestwhite-epd): the X4 Pro's UC8279
// and UC8179 e-paper controllers, black and white, the UC8179's grayscale passes, and the
// controller probe. Only test/epd_panels and test/epd_probe call it for now: the firmware's display
// stays freeink-sdk's FreeInkDisplay, and the bare-metal firmware uses the crate directly.
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// The controller's lines; every callback is required (a bus missing one is refused with no
// traffic), must not throw, and the struct must not change during a call. write sends len bytes
// inside the current chip-select frame and returns once they are out (false when it fails);
// set_cs, set_dc and set_rst drive their line; busy_high reads BUSY (low while the controller
// works: a BUSY stuck low blocks the call for good); pause_ms pauses.
typedef struct {
  void* context;
  bool (*write)(void* context, const uint8_t* bytes, size_t len);
  void (*set_cs)(void* context, bool high);
  void (*set_dc)(void* context, bool high);
  void (*set_rst)(void* context, bool high);
  bool (*busy_high)(void* context);
  void (*pause_ms)(void* context, uint32_t ms);
} pw_epd_bus;

// The controller's state between calls, opaque. Call pw_uc8279x4_init before anything else: any
// other contents, zero bytes included, are not a panel. One call at a time per panel, never from
// inside a bus callback.
typedef struct {
  uint64_t opaque[8];
} pw_uc8279x4;

// What a refresh does with the OLD plane: a flash from white, from the target's complement, or a
// partial refresh (a full one until the panel is known).
enum { PW_EPD_FULL = 0, PW_EPD_HALF = 1, PW_EPD_FAST = 2 };

void pw_uc8279x4_init(pw_uc8279x4* panel);
// Resets and programs the controller; false when a transfer fails. After any false result the
// controller's state is unknown: begin again before the next display. NULL panel or bus, or a bus
// missing a callback, is false with no traffic, as in the calls below.
bool pw_uc8279x4_begin(pw_uc8279x4* panel, const pw_epd_bus* bus);
// Shows frame (a bit per pixel, 1 white, 100-byte rows, 480 of them; unchanged until the call
// returns) and powers the panel off afterwards when asked; false when a transfer fails, or with
// no traffic when frame is NULL or too short or refresh unknown. The first display after begin
// always flashes.
bool pw_uc8279x4_display(pw_uc8279x4* panel, const pw_epd_bus* bus, const uint8_t* frame, size_t len, uint8_t refresh,
                         bool power_off);
// Powers off when on, then deep sleep, which only begin (a hardware reset) ends; false when a
// transfer fails.
bool pw_uc8279x4_deep_sleep(pw_uc8279x4* panel, const pw_epd_bus* bus);
// The next PW_EPD_FAST flashes from white instead (a PW_EPD_HALF stays one) / cancels such a
// request made after a display; the first display after begin flashes regardless.
void pw_uc8279x4_request_resync(pw_uc8279x4* panel);
void pw_uc8279x4_skip_initial_resync(pw_uc8279x4* panel);

// The UC8179 (purestwhite-epd's uc8179, freeink-sdk's Uc8179Driver): the same calls, contracts and
// frame layout as the UC8279's above, on its own state. A pw_uc8279x4 is not a pw_uc8179: passing,
// casting or copying one driver's state to the other's calls is undefined behaviour.
typedef struct {
  uint64_t opaque[8];
} pw_uc8179;

void pw_uc8179_init(pw_uc8179* panel);
// As pw_uc8179_init, with base for the grayscale base (freeink-sdk's PSRAM copy): true when it is
// at least a frame long; a shorter one is zeroed and not used (false, initialized without one), and
// NULL base is pw_uc8179_init (false). After true, base belongs to the panel until it is initialized
// again: nothing else may read or write it, and the panel must not be copied (a copy would share
// it). NULL panel: false, nothing done.
bool pw_uc8179_init_with_gray_base(pw_uc8179* panel, uint8_t* base, size_t len);
bool pw_uc8179_begin(pw_uc8179* panel, const pw_epd_bus* bus);
// freeink-sdk's blocking display: after an OVERLAY or ABSOLUTE gray refresh the next display, if
// it is a PW_EPD_FAST one (with no resync or NULL-bw cleanup between), is the non-flashing
// transition from the previous base instead.
bool pw_uc8179_display(pw_uc8179* panel, const pw_epd_bus* bus, const uint8_t* frame, size_t len, uint8_t refresh,
                       bool power_off);
// As pw_uc8179_display, never the transition: the SDK's displayStart and displayFinish, which its
// asynchronous refresh uses.
bool pw_uc8179_display_plain(pw_uc8179* panel, const pw_epd_bus* bus, const uint8_t* frame, size_t len, uint8_t refresh,
                             bool power_off);
bool pw_uc8179_deep_sleep(pw_uc8179* panel, const pw_epd_bus* bus);
// Also ends a grayscale pass under way.
void pw_uc8179_request_resync(pw_uc8179* panel);
void pw_uc8179_skip_initial_resync(pw_uc8179* panel);

// A grayscale pass (freeink-sdk's GrayscaleMode), as (LSB, MSB) plane bits per pixel. OVERLAY:
// masks over the black-and-white base, white and black (0,0), light (0,1), dark (1,1). ABSOLUTE:
// every pixel, black (0,0), dark (1,0), light (0,1), white (1,1). DIRECT: as ABSOLUTE, with the
// full-gray waveform and no base.
enum { PW_EPD_OVERLAY = 0, PW_EPD_ABSOLUTE = 1, PW_EPD_DIRECT = 2 };
// The pass: pw_uc8179_begin_grayscale, both planes, pw_uc8179_display_gray, then
// pw_uc8179_cleanup_grayscale. Frames and planes are as display's (a frame long, unchanged during
// the call); a NULL or short one is false with no traffic (except DIRECT's frame and cleanup's bw,
// which may be NULL), as are an unknown mode or refresh and the NULL cases of the calls above.
//
// Starts a pass: shows frame as its base (fallback as display's refresh, so a PW_EPD_FAST may be
// the transition), or for DIRECT programs the full-gray waveform and shows nothing (frame is not
// used). A refused frame still ends the pass under way and starts this one, as the SDK's NULL frame
// does: after ABSOLUTE the planes are taken as absolute.
bool pw_uc8179_begin_grayscale(pw_uc8179* panel, const pw_epd_bus* bus, const uint8_t* frame, size_t len, uint8_t mode,
                               uint8_t fallback, bool power_off);
// Shows frame as a base, as pw_uc8179_begin_grayscale does, without starting or ending a pass.
bool pw_uc8179_display_grayscale_base(pw_uc8179* panel, const pw_epd_bus* bus, const uint8_t* frame, size_t len,
                                      uint8_t fallback, bool power_off);
bool pw_uc8179_copy_grayscale_lsb(pw_uc8179* panel, const pw_epd_bus* bus, const uint8_t* plane, size_t len);
bool pw_uc8179_copy_grayscale_msb(pw_uc8179* panel, const pw_epd_bus* bus, const uint8_t* plane, size_t len);
// The gray refresh. A DIRECT pass needs both planes (else nothing is sent) and powers off when
// asked; the others stay powered unless factory (an image) and power_off, and an ABSOLUTE pass with
// factory runs its waveform 25% longer.
bool pw_uc8179_display_gray(pw_uc8179* panel, const pw_epd_bus* bus, bool power_off, bool factory);
// Ends the pass: both planes back to bw (the black-and-white frame shown) unless they hold it
// already. NULL bw, or after a DIRECT pass: the next refresh flashes.
bool pw_uc8179_cleanup_grayscale(pw_uc8179* panel, const pw_epd_bus* bus, const uint8_t* bw, size_t len);

// The controller probe (purestwhite-epd's probe, freeink-sdk's XteinkDetect X4-family path): the
// panel lines as GPIOs. mode, write and read are Arduino's pinMode, digitalWrite and digitalRead
// on one of the lines below; release_reset_hold releases a pad hold on reset; pause_us and
// pause_ms pause. Every callback is required and must not throw; the struct must not change during
// the call.
enum { PW_EPD_SCLK = 0, PW_EPD_SDA = 1, PW_EPD_CS = 2, PW_EPD_DC = 3, PW_EPD_RST = 4, PW_EPD_BUSY = 5 };
enum { PW_EPD_OUTPUT = 0, PW_EPD_INPUT = 1, PW_EPD_INPUT_PULLUP = 2 };
typedef struct {
  void* context;
  void (*mode)(void* context, uint8_t line, uint8_t mode);
  void (*write)(void* context, uint8_t line, bool high);
  bool (*read)(void* context, uint8_t line);
  void (*release_reset_hold)(void* context);
  void (*pause_us)(void* context, uint32_t us);
  void (*pause_ms)(void* context, uint32_t ms);
} pw_epd_probe_lines;

// What the probe read: verdict (as the SDK's DisplayControllerVerdict: 0 neither pass matched and
// no MTP confirmed, 1 a UC81xx confirmed, 2 inconclusive: exactly one pass matched, or both
// matched with different VER), VER from the authoritative pass, FLG from the first, the MTP block when
// read, and the controller the X4 Pro runs (0 SSD1677, 1 UC8179, 2 UC8279) with its LUT_VER.
typedef struct {
  uint8_t verdict;
  uint8_t ver[5];
  uint8_t flg;
  bool mtp_valid;
  uint8_t mtp[48];
  uint8_t controller;
  uint8_t lut_ver;
} pw_epd_probe_result;

// Runs the probe and leaves every line an input; out is written whole (it need not be initialized).
// False with no line touched when lines, a callback or out is NULL.
bool pw_epd_probe(const pw_epd_probe_lines* lines, pw_epd_probe_result* out);

#ifdef __cplusplus
}
#endif
