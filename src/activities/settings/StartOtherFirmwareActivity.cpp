#include "StartOtherFirmwareActivity.h"

#include <Arduino.h>
#include <GfxRenderer.h>
#include <I18n.h>

#include <string>

#include "MappedInputManager.h"
#include "activities/util/ConfirmationActivity.h"
#include "components/UITheme.h"
#include "fontIds.h"

void StartOtherFirmwareActivity::onEnter() {
  Activity::onEnter();
  if (!other_firmware::find(info)) {
    cancel();
    return;
  }
  char version[33];
  other_firmware::version(info, version, sizeof(version));
  const std::string body = version[0] ? std::string("CrossPoint ") + version : std::string();
  startActivityForResult(
      std::make_unique<ConfirmationActivity>(renderer, mappedInput, tr(STR_START_OTHER_FIRMWARE_PROMPT), body),
      [this](const ActivityResult& result) {
        if (result.isCancelled) {
          cancel();
          return;
        }
        {
          RenderLock lock(*this);
          state = State::CHECKING;
        }
        requestUpdateAndWait();
        if (!other_firmware::select(info)) {
          RenderLock lock(*this);
          state = State::FAILED;
          requestUpdate();
          return;
        }
        {
          RenderLock lock(*this);
          state = State::RESTARTING;
        }
        requestUpdateAndWait();
        delay(1500);
        ESP.restart();
      });
}

void StartOtherFirmwareActivity::cancel() {
  ActivityResult result;
  result.isCancelled = true;
  setResult(std::move(result));
  finish();
}

void StartOtherFirmwareActivity::loop() {
  if (state != State::FAILED) return;
  int x = 0;
  int y = 0;
  if (mappedInput.wasPressed(MappedInputManager::Button::Back) ||
      mappedInput.wasPressed(MappedInputManager::Button::Confirm) || mappedInput.wasScreenTapped(x, y)) {
    cancel();
  }
}

void StartOtherFirmwareActivity::render(RenderLock&&) {
  if (state == State::CONFIRMING) return;
  const auto& metrics = UITheme::getInstance().getMetrics();
  const auto pageWidth = renderer.getScreenWidth();
  const auto pageHeight = renderer.getScreenHeight();
  renderer.clearScreen();
  GUI.drawHeader(renderer, Rect{0, metrics.topPadding, pageWidth, metrics.headerHeight}, tr(STR_START_OTHER_FIRMWARE));
  const auto lineHeight = renderer.getLineHeight(UI_10_FONT_ID);
  const auto top = (pageHeight - lineHeight) / 2;
  if (state == State::CHECKING) {
    renderer.drawCenteredText(UI_10_FONT_ID, top, tr(STR_VALIDATING_FIRMWARE));
  } else if (state == State::RESTARTING) {
    const Rect bounds{metrics.contentSidePadding, top, pageWidth - metrics.contentSidePadding * 2, pageHeight - top};
    UITheme::drawCenteredWrappedText(renderer, bounds, UI_10_FONT_ID, tr(STR_RESTARTING_HINT), 3, true,
                                     EpdFontFamily::REGULAR, UITheme::TextVerticalAlignment::TOP);
  } else {
    renderer.drawCenteredText(UI_10_FONT_ID, top, tr(STR_OTHER_FIRMWARE_DAMAGED), true, EpdFontFamily::BOLD);
    const auto labels = mappedInput.mapLabels(tr(STR_BACK), "", "", "");
    GUI.drawButtonHints(renderer, labels.btn1, labels.btn2, labels.btn3, labels.btn4);
  }
  renderer.displayBuffer();
}
