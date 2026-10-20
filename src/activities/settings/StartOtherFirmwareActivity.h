#pragma once

#include "activities/Activity.h"
#include "network/OtherFirmware.h"

// Asks to restart into the firmware in the other OTA slot (see OtherFirmware.h),
// checks it and restarts. Finishes cancelled when there is none or the user
// declines. Used from Settings -> System and from boot recovery mode.
class StartOtherFirmwareActivity : public Activity {
 public:
  StartOtherFirmwareActivity(GfxRenderer& renderer, MappedInputManager& mappedInput)
      : Activity("StartOtherFirmware", renderer, mappedInput) {}

  void onEnter() override;
  void loop() override;
  void render(RenderLock&&) override;
  bool preventAutoSleep() override { return state == State::CHECKING; }

 private:
  enum class State { CONFIRMING, CHECKING, RESTARTING, FAILED };

  State state = State::CONFIRMING;
  other_firmware::Info info;

  void cancel();
};
