# Emülatör ve HAL envanteri

Aşama 6'nın (HAL ve SDK sürücüleri Rust'a) güvenlik ağı `emu/compare.sh` olacak: aynı senaryoyu
eski ve yeni yapıda eink-qemu'da koşturup ekranları karşılaştırır. Bu belge HAL'in her parçası için
emülatörün neyi gerçekten modellediğini, neyin eksik olduğunu ve neyin her zaman cihazda
denenmesi gerektiğini tutar (2026-10-25).

eink-qemu üçüncü taraf bir projedir: `serialx/eink-qemu` (GPLv2, QEMU 11.1.2 tabanlı), biz
yalnızca sürüm arşivlerini kullanıyoruz (`../tools/eink-qemu-0.7.0`). Kendi kuralı "donanımı
veri sayfasından ve karttan modelle, tek bir yazılımın sürücüsünden değil"dir; bu yüzden
eklenecek bir model bizim sürücümüzü değil, X4 Pro'nun donanımını taklit etmelidir.

## Parça parça

| HAL parçası | X4 Pro donanımı | eink-qemu'da | Bizim senaryolarımız | HAL taşıması için eksik |
| --- | --- | --- | --- | --- |
| `HalDisplay` | UC8279 (0x68) panel, GPSPI2 + GDMA, BUSY | Var: panel komutları, kısmi pencere (0x90), BUSY süresi, yenileme sayısı (`refreshes`, `refresh-ns`); hayalet görüntü isteğe bağlı | Hepsi (her `shot`) | Yok. Dalga biçimi ve hayalet görüntünün görünüşü cihazda |
| `HalGPIO`: düğmeler | Yukarı GPIO0, Aşağı GPIO7, Güç GPIO3 | Var (`press:up/down/power`, uzun basış) | `reading`, `sleep`, `open-book`... | Yok |
| `HalGPIO`: dokunma | GT911 (I2C 0x5D, INT GPIO10, güç GPIO2), kapasitif Home | Var (`touch`, `swipe`, `press:home`); çoklu dokunma yok | `ble-settings`, `settings`, `library` | Yok (çoklu dokunma kullanılmıyor) |
| `HalStorage` | SDMMC 1 bit, GPIO5 güç | Var: FAT32 kart, okuma ve yazma; hız X3'te ölçülmüş | Kitap ve kart kopyalayan her senaryo | Kart hatası (okuma hatası, çıkarılan kart, bozuk FAT) üretilemiyor |
| `HalPowerManager`: uyku | Derin uyku, ext0/ext1 düğme ile uyanma, RTC belleği | Var: derin ve hafif uyku, zamanlayıcı ve pin uyanması, RTC belleği korunur | `sleep` | Yok. Uyanma gecikmesi ve akım cihazda |
| `HalPowerManager`: pil | CW2017 yakıt göstergesi (I2C 0x63) | Var: yüzde ve gerilim, `hmp:qom-set` ile çalışırken değişir | Durum çubuğu | Yok |
| `HalPowerManager`: şarj | STAT GPIO21 (yüksek = şarj oluyor) | **Yok**: pin hep 1 okunur ("şarjda") | Yok | Şarj durumu değişmiyor; şarjda/değil ayrımı denenemiyor |
| `HalClock` | BM8563 RTC (I2C 0x51) | Var: ana makinenin saati ya da `--rtc` (varsayılan 2026-01-01 09:00) | `clock` (başlık ve durum çubuğu saati); hepsi `--rtc` sabit | Yok |
| `HalFrontlight` | LEDC PWM, soğuk GPIO8 (kanal 4), sıcak GPIO9 (kanal 5) | **Yok** (üst projede "open") | Yok | Parlaklık ve renk sıcaklığının görev oranı okunamıyor |
| `HalHaptics` | Motor yok (yalnızca Metalio'da) | Gerekmez | — | HAL'in boş yolu ana makine testiyle |
| `HalTiltSensor` | IMU yok (`ImuType::None`) | Gerekmez | — | HAL'in boş yolu ana makine testiyle |
| `HalMemory`, `HalSystem` | PSRAM 8 MB, panik dökümü | PSRAM var (60–90 % hızlı modellenmiş); yığın yürüyüşü (`heap`) | `clipping-large` | Yok. Mutlak zamanlama cihazda |
| Wi-Fi (SDK) | ESP32-S3 Wi-Fi | Sahte açık erişim noktası (`PICSimLabWifi`), NAT ya da köprü; WPA ve sinyal yok | Yok | Senaryo yok (web sunucusu, WebDAV, OPDS, KOSync yapılabilir); WPA ve gerçek ağ cihazda |
| BLE sayfa çevirici (SDK) | NimBLE + Espressif'in kapalı denetleyicisi | Radyo açılır, tarama hiçbir şey bulmaz; bağlantı katmanı yok (üst projede yapım aşamasında) | `ble-settings` | Eşleşme ve sayfa çevirme hiçbir zaman emülatörde değil, cihazda |
| USB | USB-CDC günlük | Günlük var; USB depolama yok | — | Kullanılmıyor |

## Bilinen emülatör sınırları ve sürüm

`emu/compare.sh` 0.9.2'yi kullanır (`../tools/eink-qemu-0.9.2`; 0.7.0 yanında duruyor).
Aradaki sürümler HAL işi için önemli:

- **0.9.1**: X4 Pro'da OTA yuvası değiştikten sonraki yeniden başlatma aynı oturumda açılıyor
  (yalnızca CPU'yu sıfırlayan yeniden başlatmalar diğer çekirdeği ve paylaşılan çevre birimlerini
  koruyor). 0.7.0'da `ESP.restart` her zaman takılıyordu; güncelleme ve "Diğer yazılımı başlat"
  akışları emülatörde denenemiyordu. Denendi: Wi-Fi oturumundan çıkıştaki `silentRestart`
  (`RTC_SW_CPU_RST`) 0.7.0'da ROM'da döngüye giriyor, 0.9.2'de Home'a açılıyor
  (`emu/restart.txt`).
- **0.9.0**: betikli koşularda `--sleep off` varsayılan: konuk boştayken saati bir sonraki
  zamanlayıcıya atlar. Bu kipte `swipe` konuk zamanında uzar ve ürün yazılımı onu kaydırma saymaz
  (sayfa çevrilmez); `compare.sh` bu yüzden `--sleep on` geçer.
- **0.8.0**: Wi-Fi'de NAT ve köprü arasında açık seçim.

## Her zaman cihazda

Emülatör bunları hiçbir zaman tam doğru vermez; HAL taşımasında da `DONANIM-TESTLERI.md`'ye cihaz
testi olarak girerler:

- Mutlak zamanlama: e-ink yenileme süreleri, PSRAM'a dayanan yollar, derin uykudan uyanma gecikmesi.
- Güç: pil tüketimi, uyku akımı, şarj.
- Gerçek SD kart davranışı: yavaş ya da bozuk kartlar, takıp çıkarma.
- Ön ışığın görünüşü, hayalet görüntünün görünüşü.
- Gerçek Wi-Fi (WPA, zayıf sinyal, kopma) ve BLE eşleşmesi.

## Önerilen sıra

HAL'i taşıma sırasına göre (önce emülatörde tam görülenler):

1. **eink-qemu'yu 0.9.2'ye yükseltmek ✅**: 23 senaryo iki yapıda da geçiyor ve ekranları aynı;
   yeniden başlatan akış `emu/restart.txt` ile senaryoya girdi.
2. **Saf parçalar** (`HalClock` ✅ `purestwhite-rtc`, `HalMemory`, `HalSystem`): emülatör yeterli.
3. **`HalStorage`**: emülatör mutlu yolu tam gösterir; hata yolları ana makine testleriyle
   (sahte dosya sistemi), çünkü emülatörde kart hatası üretmek QEMU'nun SD modelini değiştirmeyi
   gerektirir.
4. **`HalGPIO`** (düğmeler, dokunma) ve **`HalDisplay`**: emülatör tam.
5. **`HalPowerManager`**: uyku ve pil emülatörde; şarj pini için emülatöre bir model eklemek
   gerekir (küçük: GPIO21'i dışarıdan sürülebilir yapmak).
6. **`HalFrontlight`**: emülatöre LEDC modeli eklemek gerekir (orta büyüklükte: iki kanalın görev
   oranını okunur bir özellik olarak vermek), yoksa yalnızca cihazda.
7. **Wi-Fi ve BLE** (SDK'nın parçaları): Wi-Fi için sahte erişim noktasıyla web senaryoları
   yazılabilir; BLE yalnızca cihazda.

Şarj pini ve LEDC için emülatöre ekleme, eink-qemu'nun kaynağından kendi yapımızı derlemeyi (ya da
üst projeye katkı vermeyi) gerektirir. Bu, sahibinin kararıdır.
