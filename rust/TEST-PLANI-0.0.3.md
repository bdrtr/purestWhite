# 0.0.3 sürümü için cihaz testi planı

`rust/DONANIM-TESTLERI.md`'de işaretlenmemiş 255 madde var. Bu plan onları cihazda aynı oturumda
denenebilecek gruplara ayırır; her grup, maddelerin geldiği DONANIM-TESTLERI bölümlerinin adlarını
verir. Bir madde denendiğinde DONANIM-TESTLERI'nde `[x]` yapılır; bir sorun çıkarsa o maddenin altına
not düşülür. Süre ölçümü isteyen maddeler sonda, ayrı bir oturumda.

Gruplar önem sırasında: ilk dördü günlük kullanımın tamamını kapsar, sorun çıkarsa en çok onlarda
çıkar.

## Emülatörde 0.0.2 ile karşılaştırma

Sürümden önce bütün emülatör senaryoları `emu/compare.sh` ile 0.0.2'ye (f3776317) karşı çalıştırıldı;
22 senaryonun 124 ekran görüntüsü birebir aynı: ayarlar, BLE ayarları, okuma (Çiçekli Şiir, CSS,
iki yönlü, heceleme kitapları), XML kitabı, kitap açma, inflate, JPEG, bozuk JPEG, CSS boyutlu
JPEG, PNG, bozuk PNG, BMP görüntüleyici, saydam uyku kaplaması (BMP ve PNG), sözlük (düz metin ve
HTML/dictzip), alıntı ekleme ve silme, kütüphane, XTC. Emülatörün göremediği şeyler (ağ, BLE,
gerçek SD kart hızı, dokunmatik sürüklemeler, gerçek kitaplar) aşağıdaki oturumlarda.

## 0. Hazırlık

- **Yedek.** Şu anki yazılım diğer yuvada kalır (SD kart güncellemesi onu silmez), yine de
  `~/Documents/bembeyaz/firmware-arsiv/`'deki son sürümü kartta tutun.
- **Yükleme.** `purestWhite-next-0.0.3.bin`'i kartın köküne koyun, Ayarlar → Sistem → SD Kart
  Yazılım Güncellemesi ile seçin. Hakkında'da "purestWhite next 0.0.3" görünmeli; Start Other
  Firmware önceki yazılımı adıyla göstermeli.
- **Test kartı.** `~/Documents/bembeyaz/test-karti-0.0.3/` klasörünün içindekileri kartın köküne
  kopyalayın (ayar dosyası içermez, ayarlarınız değişmez):
  - `kitaplar/`: emülatör senaryolarının kitapları (CSS, iki yönlü, heceleme, XML, inflate, JPEG,
    CSS boyutlu JPEG, PNG, sözlük kitabı, Çiçekli Şiir) ve `test/epubs/`'taki deneme kitapları
    (tablolar, üst/alt simge, resimler, kerning…); `kitaplar/bozuk/` bozuk dosyalar.
  - `resimler/`: 1, 2, 4, 8, 24 ve 32 bitlik BMP'ler ve kesik `7-cut.bmp`.
  - `kaplama/`: saydam uyku kaplamaları (`sleep-overlay.bmp`, `sleep-overlay.png`); denerken
    birini köke taşıyın, ikisi birden kökte olmasın (BMP önce seçilir).
  - `xtc/`: 1 ve 2 bitlik XTC kitapları.
  - `.dictionaries/plain` ve `.dictionaries/html`: düz metin ve HTML (dictzip) deneme sözlükleri.
  - `.fonts/LiberationSerif/`: SD kart fontu.
- **Kendi kitaplarınız.** Bazı maddeler gerçek kitap ister: Türkçe ve İngilizce romanlar, bir EPUB 2
  (yalnızca NCX), dipnotlu bir kitap, tablolu bir kitap, Japonca/Çince (ruby'li), Arapça ya da
  İbranice, yüzlerce bölümlü büyük bir kitap, TXT ve Markdown birer kitap, alıntısı olan bir kitap.
  Bunların bir kısmını önceki sürümde açmış olmanız iyi olur (önbellek uyumluluğu için).
- **Seri günlük.** Günlük satırı isteyen maddeler geliştirme derlemesi (`pio run -e x4pro`) ve
  `pio device monitor` gerektirir; USB bağlantısı yalnızca hub üzerinden çalıştığı için bunlar
  sonraya kalabilir (6. oturum).

## 1. Önbellekler ve ilk açılış

Önceki sürümde açılmış kitapları açın, sonra birini önbelleğini silerek yeniden açın.

- Önceki sürümle açılmış kitaplar yeniden dizilmeden açılmalı, okuma konumu korunmalı.
- Önbelleği silinmiş kitapta içindekiler, bölüm adları ve "%'ye git" doğru olmalı.
- Uzun bir bölümün dizilmesi sürerken çıkıp yeniden girince kaldığınız sayfa hemen açılmalı.
- Yazı boyutunu değiştirip geri dönün: bölüm yeniden dizilmeli, eski boyutta da doğru görünmeli.

DONANIM-TESTLERI bölümleri: İlk açılış; Bölüm dosyası tabloları; Bölüm sayfaları; Kitap üst veri
önbelleği; Bozuk önbellek dosyası çökertmiyor; container.xml; OPF; İçindekiler okuyucuları;
İçindekiler etiketi; ROM'daki inflate (1. madde); Sıkıştırma hızı.

## 2. Okuma ve sayfa düzeni

`kitaplar/css-test.epub` (Gömülü Stil açık), kendi romanlarınız ve dipnotlu kitabınızla.

- **Stiller.** Hizalama, girintiler, kenar boşlukları, başlık boyutları, kalın/italik/çizgiler,
  üst ve alt simge, küçük büyük harf, listeler (iç içe numaralı dahil), `pre`, `<hr>`, gizli
  öğeler; çerçeveli ve gri kutular (birden çok sayfaya taşan kutuda kesilen kenarlar açık).
- **Büyük ilk harf.** Boyut, üst ve taban hizası, yanındaki satırların girintisi; yazı boyutunu ve
  tipini değiştirerek; sayfa sonuna denk gelen büyük ilk harfli paragraf.
- **Sayfa sonları.** Yalnız kalan ilk/son satırlar ve sayfa altındaki başlıklar sonraki sayfaya
  geçmeli; dipnot bağlantılı satır taşındığında dipnot doğru sayfanın listesinde olmalı.
- **Bağlantılar.** Dipnot ve iç bağlantılar doğru yere gitmeli (Dokunmatik okuyucu kontrolleri
  açıkken dokunarak); "Bağlantılar ve dipnotlar" listesinde gezinince iz kalmamalı.
- **Tablolar.** `crosspoint-table-edge-cases.epub` ve `test_tables.epub`; dikey ve yatay ekranda,
  büyük yazıda.
- **Odaklanmış Okuma**, heceleme açık/kapalı, kırılmaz boşluklu (Fransızca) bir kitap, TXT ve
  Markdown.

DONANIM-TESTLERI bölümleri: XML ayrıştırıcı; Yol çözümü; Paragraf yerleşimi; CSS; Bölüm metni
sözcüklere bölünür; Büyük ilk harfin yakalanması; Satır içi stil yığını; Satır içi yazı boyutu;
Bloğu tutan span'ın boyutu; Blok stili; Öğe sınıflandırması; Sayfa sonunda taşınan satırlar; Tablo
sütun genişlikleri; Tablo satırı geometrisi; Tek tek denetlenen etiket adları; Liste işaretleri ve
ruby metni; Sayfa bağlantılarının boş baytları; Çerçeveli kutular; Büyük ilk harfin yerleşimi;
Sayfa sonunda taşınan bağlantılar ve dipnotlar; TXT ve Markdown.

## 3. Diller ve yazı tipleri

- Türkçe, İngilizce ve Almanca kitaplar heceleme açıkken; `hyphenation-test.epub`.
- Japonca/Çince (ruby'li), Korece, Arapça ya da İbranice (`bidi-test.epub` da).
- Yerleşik yazı tipleri dört boyutta, kalın/italik, "fi"/"fl" bağları; dört Okuma Yönü; Metin
  Yumuşatma açık ve kapalı.
- `.fonts/LiberationSerif` okuma fontu olarak: kerning çiftleri ("To", "AV"), kalın/italik,
  yeniden açınca yeniden dizilmeme; varsa CJK ve ligatürlü bir SD fontu.

DONANIM-TESTLERI bölümleri: Heceleme; Çift yönlü metin; DEFLATE ve yazı tipleri; SD kart fontunun
başlığı, kerningi, glif okuyucuları, aralık tablosu, sayfa kümesi, ilerleme genişliği tablosu;
Bitmap'i boyutundan kısa SD font glifleri; Yerleşik sıkıştırılmış fontlar; Glifler; Ölçeklenmiş
glifler; Çizim temelleri.

## 4. Resimler, kapaklar ve uyku ekranı

- **Bölüm resimleri.** `jpeg.epub` (11 bölüm), `png.epub` (13 bölüm), `jpeg-sized.epub` (CSS ile
  boyutlanmış resimler), `inflate.epub`, `test_mixed_images.epub`; ilk açılışta ve önbellekten,
  gece modunda, dört yönelimde. Görünüşleri emülatör raporlarıyla karşılaştırın
  (`emu/runs/jpeg-jpeg-*`, `png-png-*`, `jpeg-sized-*`).
- **Bozuk resimler.** `kitaplar/bozuk/jpeg-broken.epub` ve `png-broken.epub` donmadan açılmalı.
- **Resim görüntüleyici.** `resimler/` altındaki BMP'ler (gece modu açık/kapalı; `7-cut.bmp` kısmen),
  bir JPEG ve bir PNG; Kapak Ayarla.
- **Kapaklar.** JPEG ve PNG kapaklı kitaplar (ilk kez açılan): Home küçük resimleri tüm temalarda
  (Klasik, Lyra, Lyra Genişletilmiş, RoundedRaff, Cover Grid), gece modunda; kapaksız kitabın
  simgesi ve yer imi şeridi; TXT'nin yanındaki aynı adlı PNG.
- **Uyku ekranı.** Kapak (Sığdır ve Kırp), Özel (`/sleep.bmp` ve `/sleep/` klasörü; filtreler),
  Transparent (`kaplama/` altındaki BMP ve PNG; okurken, döndürülmüşken ve Home'dayken), papatya
  logosu.

DONANIM-TESTLERI bölümleri: PNG resimleri; JPEG resimleri; Bozuk JPEG; PNG kapakları; PNG kapak bit
derinliği; BMP okuyucusu; Saydam uyku kaplaması; Logo; Yuvarlatılmış dikdörtgenler; Dolu
çokgenler; Resimler; Ekran bölgesi kopyaları; Alttan başlayan resimler; Titreme; Önbellekteki
resim satırları; JPEG ve PNG resimlerinin çizimi; PNG satırlarının griye çevrilmesi; Kapak
satırlarının ölçeklenmesi; Resim boyutu yoklaması; PNG kapak okuyucusunda bölmesiz örnekler; Resim
boyutu.

## 5. Sözlük, kütüphane, alıntılar, XTC

- **Sözlük.** Sözlük ayarında önce `plain`, sonra `html`; `dict-book.epub`'ta "Stories", "walked",
  "“zebra”", "cat’s", "colour" (eş anlamlı), "running", "quixotic" (16 KiB'den uzun tanım) ve
  "qwertyuiop" (bulunamaz). İlk aramada "Sözlük endeksleniyor..." görünmeli. Varsa gerçek bir
  StarDict sözlüğü (HTML tanımlı ve `.dict.dz`), önceki sürümün endekslediği bir sözlük.
- **Kütüphane.** Başlık/Yazar/Son sekmeleri, harf grupları, "cicek" araması, yazar adlarının
  birleşmesi, yenileme; yüzlerce kitaplı bir kart.
- **Alıntılar.** Önceki sürümün alıntıları; ekleme, silme, yeniden açma; varsa alıntı eşitlemesi.
- **XTC.** `xtc/` altındaki iki kitap, bölüm listesi, kapak küçük resmi.

DONANIM-TESTLERI bölümleri: Sözlük dizini ve aramaları; Sözlük HTML tanımları; HTML'in düz metne
çevrilmesi; Dictzip; Kütüphane metni; Kütüphane dizin dosyası; Kütüphane dizini yazıcısı; Alıntı
deposu; Alıntı metinleri; Alıntı dosyası; XTC/XTCH ayrıştırıcısı; XTC bölüm listesi.

## 6. Ağ, eşitleme, güncelleme

Wi-Fi'ye bağlı, saati doğru bir cihazla.

- **Ad.** Yönlendiricide "purestWhite-<MAC>", `http://purestwhite.local`, Calibre Wireless.
- **Dosya aktarımı.** Web sunucusu ve WebDAV ile alt klasöre yükleme, listeleme, indirme.
- **OPDS.** OPDS 1 ve 2 kataloglarda gezinme, arama, indirme; Ayrıntılar ekranında JPEG/PNG kapak
  ve HTML açıklama (uzun açıklama dahil); varsa oturum açılan katalog.
- **Plugin Hub, Manage Fonts, LCP** kitabı.
- **Sertifikalar.** badssl.com adresleri, kendinden imzalı ev sunucusu (ayar açık ve kapalı).
- **KOSync.** Varsayılan sunucu, sync.koreader.rocks ve `kosync.ak-team.com:3042`; Smart Sync ve
  Ask Every Time; "resumed" satırları; KOReader ile iki yönlü konum (paragraf ortası, liste
  maddesi, bölüm sonu, büyük resimli bölüm).
- **Güncelleme.** 0.0.3 GitHub'da yayımlandıktan sonra 0.0.2 çalışan cihazda Check for updates 0.0.3'ü
  önermeli; aynı sürümde önermemeli; indirme sırasında Wi-Fi kesilince eski sürüm açılmalı.

DONANIM-TESTLERI bölümleri: Ad; XML ayrıştırıcı (OPDS, WebDAV, KOReader maddeleri); Yol çözümü
(WebDAV); Sunucu sertifikası doğrulaması; Saat eşitlemesi; TLS oturum sürdürme; KOSync tek
bağlantı; Gövdesiz yanıtlar; Wi-Fi ile güncelleme; JSON ayrıştırıcı; Uzun OPDS açıklamaları; Sürüm
0.0.2; KOReader bölüm konumları ve konum bulma.

## 7. BLE kumandalar

Dört kumanda eşleme, beşinci için uyarı; bir kumanda bağlıyken PNG'li bir bölümü ve önbelleği
silinmiş bir kitabı ilk kez açma, sözlükte arama.

DONANIM-TESTLERI bölümleri: BLE kumandalar; PNG resimleri (BLE maddesi); Hizalı Rust blokları; PNG
kapakları (BLE maddesi); ROM'daki inflate (BLE maddesi).

## 8. Seri günlük ve süre ölçümleri

Geliştirme derlemesi ve `pio device monitor` ile; önceki sürümle aynı kitaplarda karşılaştırın.
Belirgin bir yavaşlama ya da DONANIM-TESTLERI'nde adı geçen hata satırlarından biri görülürse not
edin.

- `Time to parse and build pages` (büyük kitap, SD fontlu kitap), `PNG/JPEG decoding complete -
  render time`, kapak dönüştürme süreleri, `Page render:` süreleri (dikey ve yatay), alıntı ekleme
  ve silme süresi (çok alıntılı kitap), kütüphane yenileme süresi.
- Yazı tipi, bölüm, kitap önbelleği, alıntı ve kütüphane hata satırlarının görünmemesi.

DONANIM-TESTLERI bölümleri: DEFLATE ve yazı tipleri; Sıkıştırma hızı; PNG ve JPEG resimleri (hız
maddeleri); PNG kapakları (hız); SD kart fontu bölümlerinin günlük maddeleri; Yerleşik sıkıştırılmış
fontlar (günlük); Bölüm dosyası tabloları ve Bölüm sayfaları (günlük); Kitap üst veri önbelleği
(günlük); Alıntı bölümleri (süre); Kütüphane dizini yazıcısı (süre); Glifler (süre).

## 9. Bozuk dosyalar ve kurtarma

- `kitaplar/bozuk/` altındaki kitaplar ve bir hex düzenleyiciyle ortası bozulmuş bir EPUB: donma ya
  da yeniden başlatma olmamalı.
- Yukarı tuşuyla açılışta donan ekrandan nasıl çıkıldığını not edin; Aşağı + Güç ile kurtarma kipi.

DONANIM-TESTLERI bölümleri: Sıkıştırma; ROM'daki inflate (bozuk EPUB); İki yazılım yan yana.

Kendiliğinden geçen maddeler: "Hizalı Rust blokları"nın ilk maddesi ve "PNG kapak bit derinliği"
cihazda ayrıca bir şey istemez; diğer oturumlarda `Rust panic`, yeniden başlatma ya da donma
görülmezse işaretlenebilir.
