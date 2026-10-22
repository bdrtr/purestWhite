# Rust'a kademeli geçiş

CrossPoint çalışan bir firmware olarak kalır; modüller birer birer Rust'a taşınır.
Her adımda cihaz açılır, host testleri geçer, kullanıcı geri dönebilir.

## Yöntem

1. Bir C++ modülü seçilir (bağımlılık grafiğinde alttan yukarı).
2. Mantığı `rust/crates/bembeyaz-<modül>` altında, donanımdan bağımsız
   `no_std` bir crate olarak yazılır ve `cargo test` ile birim testleri eklenir.
3. `bembeyaz-ffi` bunu `bb_<modül>_*` C fonksiyonları olarak dışa açar;
   bildirimleri `rust/include/bembeyaz/<modül>.h` içindedir.
4. C++ header'ının public API'si **değişmez**; `.cpp` dosyası Rust'ı çağıran ince bir shim olur.
   Böylece çağıran kod dokunulmadan kalır.
5. Mevcut host testleri (`test/`, gtest) artık Rust'ı test eder. Davranış farkı
   varsa burada yakalanır. Testler geçince eski C++ gövdesi silinir.

Shim'ler, çağıranlar Rust'a geçtikçe kaybolur. Sonunda `main` Rust'a geçer
ve Arduino katmanı kalkar.

## Derleme

- Firmware: `pio run -e x4pro` (veya `x4c`). `scripts/build_rust.py`, kartın
  MCU'suna göre `bembeyaz-ffi`'yi derleyip bağlar:
  - ESP32-S3 → `cargo +esp ... --target xtensa-esp32s3-none-elf -Zbuild-std=core,alloc`
  - ESP32-C3 → `riscv32imc-unknown-none-elf` (stable). Bu karta artık env yok;
    satır, ileride RISC-V çipli bir kart için şablon olarak duruyor.
- Host testleri: `cmake -S test -B build/test && cmake --build build/test && ctest --test-dir build/test`
  CMake, Rust kütüphanesini host için derleyip her teste bağlar.
- Yalnız Rust: `cd rust && cargo test`

## Emülatörde doğrulama

[eink-qemu](https://github.com/serialx/eink-qemu) X4 Pro'yu (ekran, SDMMC,
GT911, CW2017, deep sleep) emüle eder. `emu/compare.sh`, değiştirilmemiş
CrossPoint build'ini (`../crosspoint-base` worktree'si) ve bu ağacın build'ini
aynı senaryoyla yan yana çalıştırır: ekran görüntüsü farkı, heap, render süreleri.

```sh
pio run -e x4pro && emu/compare.sh            # emu/reading.txt senaryosu
emu/compare.sh emu/baska-senaryo.txt x4pro kitap.epub

# Inflate: PNG kapak (zlib akışı, bembeyaz-png), stored ve level-9 bölümler (InflateStream, ROM'daki miniz)
python3 emu/make_inflate_epub.py emu/books/inflate
emu/compare.sh emu/inflate.txt x4pro emu/books/inflate/inflate.epub

# Sağdan sola metin (Arapça, İbranice). Yerleşik fontlarda bu harfler yok: gövde
# kutularla çizilir, karşılaştırma satır düzenini ve başlığı ölçer.
python3 emu/make_bidi_epub.py emu/books/bidi
emu/compare.sh emu/reading.txt x4pro emu/books/bidi/bidi-test.epub

# CSS: hizalama, girinti, kenar boşlukları, kalın/italik, küçük büyük harf, kenarlıklar
python3 emu/make_css_epub.py emu/books/css
emu/compare.sh emu/reading.txt x4pro emu/books/css/css-test.epub

# Heceleme: Almanca bileşik kelimeler, tireler, kesme işaretleri; kart hecelemeyi açan ayarla gelir
python3 emu/make_hyphenation_epub.py emu/books/hyphenation
emu/compare.sh emu/reading.txt x4pro emu/books/hyphenation/hyphenation-test.epub \
    emu/books/hyphenation/card/.crosspoint=/

# XML: kapsayıcı, paket belgesi, NCX ve nav; HTML varlıkları, CDATA, tablo, UTF-16, ISO-8859-1,
# iç DTD ve 32 KB'tan uzun bir paragraf. Bozuk ikizi açılışta aynı hataları vermeli; varlık ikizinin
# ilk bölümü ve nav belgesi DTD'siz HTML varlıkları kullanır (eski sürümde bölüm açılmaz, nav yerine
# NCX okunur).
python3 emu/make_xml_epub.py emu/books/xml
emu/compare.sh emu/xml.txt x4pro emu/books/xml/xml-test.epub
emu/compare.sh emu/open-book.txt x4pro emu/books/xml/broken/xml-broken.epub
emu/compare.sh emu/open-book.txt x4pro emu/books/xml/entities/xml-entities.epub

# Bölüm resimleri (bembeyaz-png): dönüştürücünün çözdüğü her renk türü ve bit derinliği, saydamlık,
# küçültülen büyük resim; PNG kapak (Home'daki küçük resim, `cover` okuyucusu). Bozuk ikizinin
# resmi, verisinin bitimine 10 bayt kala kesik: eski sürüm onu açarken donar.
python3 emu/make_png_epub.py emu/books/png
emu/compare.sh emu/png.txt x4pro emu/books/png/png.epub
emu/compare.sh emu/png-broken.txt x4pro emu/books/png/png-broken.epub

# BMP (bembeyaz-bmp): resim görüntüleyicide 1, 2, 4, 8, 24 ve 32 bitlik, alttan ve üstten başlayan
# resimler; sonuncusunun piksel verisi yarıda kesik (iki sürüm de 199 satırdan sonra hata verir,
# günlükte `row 199`; alttaki satırlar çizilir).
python3 emu/make_bmp_files.py emu/books/bmp
emu/compare.sh emu/bmp.txt x4pro emu/books/bmp/*.bmp
# Saydam uyku kaplaması (/sleep-overlay.bmp, 32 bit BGRA): Power'a basılı tutup uyutur.
emu/compare.sh emu/sleep.txt x4pro emu/books/bmp/overlay/card/sleep-overlay.bmp \
    emu/books/bmp/overlay/card/.crosspoint=/

# XTC/XTCH (bembeyaz-xtc): 1 ve 2 bitlik iki kitap açılır, her birinde iki sayfa çevrilir.
python3 emu/make_xtc_files.py emu/books/xtc
emu/compare.sh emu/xtc.txt x4pro emu/books/xtc/book.xtc emu/books/xtc/book.xtch

# SD kart fontu (bembeyaz-font): varsayılan kitap karttaki bir .cpfont fontuyla okunur. Fontu
# lib/EpdFont/scripts/fontconvert_sdcard.py yapar (freetype-py ister); burada Liberation Serif'in
# dört stiliyle. Bozuk ikizinde ilk stilin ikinci aralığı birincisiyle çakışır: iki sürüm de fontu
# reddeder (günlükte `invalid interval layout at 1`) ve yerleşik fontla okur. Kitaptaki Korece
# harfler bu fontta yok: iyi kartta iki sürümün günlüğünde de aynı `No glyph` satırları çıkar.
f=/usr/share/fonts/liberation/LiberationSerif
python3 lib/EpdFont/scripts/fontconvert_sdcard.py --intervals latin-ext --size 14 \
    --regular $f-Regular.ttf --bold $f-Bold.ttf --italic $f-Italic.ttf \
    --bolditalic $f-BoldItalic.ttf -o emu/books/LiberationSerif_14.cpfont
python3 emu/make_sd_font_cards.py emu/books/LiberationSerif_14.cpfont LiberationSerif 14 emu/books/sdfont
book="$(python3 emu/make_utf8_epub.py emu/books)"
emu/compare.sh emu/reading.txt x4pro "$book" \
    emu/books/sdfont/good/.fonts=/ emu/books/sdfont/good/.crosspoint=/
emu/compare.sh emu/reading.txt x4pro "$book" \
    emu/books/sdfont/bad/.fonts=/ emu/books/sdfont/bad/.crosspoint=/
# Kısa bitmap'li ikizinde düz stilin "e" glifi 1 baytlık bitmap bildirir: eski sürüm "e" yerine
# bitmap'in ötesindeki baytları çizer, bu sürüm "e"yi boş bırakır (sayfa ekranları kasıtlı farklı).
emu/compare.sh emu/reading.txt x4pro "$book" \
    emu/books/sdfont/short/.fonts=/ emu/books/sdfont/short/.crosspoint=/

# JPEG (bembeyaz-jpeg): gri ton, 4:4:4/4:2:2/4:2:0, ilerlemeli, restart, CMYK, 1/2, 1/4 ve 1/8
# ölçekte çözülen büyük resimler; JPEG kapak (Home'daki küçük resim, tam boyutta çözülür). Bozuk
# ikizinin resminde bir başlık segmenti dosyanın sonunu aşıyor: eski sürüm onu açarken donar.
python3 emu/make_jpeg_epub.py emu/books/jpeg
emu/compare.sh emu/jpeg.txt x4pro emu/books/jpeg/jpeg.epub
emu/compare.sh emu/jpeg-broken.txt x4pro emu/books/jpeg/jpeg-broken.epub

# Bozuk EPUB'lar (her biri kendi kartında):
python3 emu/make_bad_epubs.py emu/books/bad
emu/compare.sh emu/open-book.txt x4pro emu/books/bad/max-size-cover/max-size-cover.epub
```

Davranışı korumak için yapılan taşımada beklenen sonuç "N of N screenshots
identical". Kasıtlı bir düzeltme fark üretiyorsa, bu fark commit mesajında
açıklanır.

Cihazda denenmeyi bekleyen değişiklikler [DONANIM-TESTLERI.md](DONANIM-TESTLERI.md) dosyasında
birikir; her commit kendi maddesini ekler.

## Kurallar

- **ESP-IDF'in sunduğunu kullanırız; fayda sağlayacaksa kendimiz yazarız.** ESP-IDF ya da
  S3 ROM'u bir işi hazır sunuyorsa (ör. ROM'daki miniz inflate: `InflateStream`) onu
  kullanırız; kendi kodumuzu yalnızca ölçülmüş bir fayda (hız, bellek, davranış) varsa,
  gerekçesiyle yazarız. Örnek: PNG çözücüleri `bembeyaz-inflate`'te kalır, çünkü satırları
  kopyalamadan doğrudan inflate penceresinden çözerler ve bozuk girdide PNGdec'le (zlib) aynı
  sonucu veren denetimler (akışın başından önceye uzanan mesafeyi ve blok sonu kodu olmayan bloğu
  reddetmek: `e734c19`, `b109503`) ROM'un miniz 1.15'inde yoktur. Firmware'e bağlanan Rust
  kodunda (`bembeyaz-ffi`'nin bağımlılıkları) üçüncü taraf crate yoktur: kendi yazdığımız
  algoritmalar standarttan (UAX #9, RFC 1951 …) en küçük ve en hızlı olacak şekilde yazılır. Testlerde
  referans olarak kullanılan crate'ler (`zip`; test verisini sıkıştıran ve
  karşılaştırma testinde `bembeyaz-inflate`'in yerini aldığı çözücü olan miniz_oxide)
  de zamanla kendi ürettiğimiz fixture'larla değiştirilir.
- **Taşıma önce eski davranışı korur.** Eski koda karşı yazılmış oracle testleri
  değişmeden geçer; standarttan sapmalar sonra, her biri kendi commit'inde düzeltilir.
- **Rust tarafı birebir çeviri değildir.** Crate'ler dilim, iterator, `Option` ve
  `Result` kullanır; NUL ile biten string, -1 gibi özel değerler ve sessiz hata yoktur.
  Sınırın C biçimini (C string, `int` dönüş kodları) FFI katmanı ve shim taşır. Dört istisna var:
  `bembeyaz-layout`: renderer ve `TextBlock` C string aldığı için sözcükleri NUL ile biten
  biçimde tutar, ölçer ve verir. `bembeyaz-json`: belirteç ayrıştırıcının tamponunda durur ve SDK
  çağıranları onu C string olarak okur, bu yüzden ardından NUL gelir; `Token::bytes()` NUL'suz
  görünümü verir. `bembeyaz-html`: varlık değerleri 4 baytlık sıfırla doldurulmuş alanlarda
  durur, C string döndüren `lookupHtmlEntity` için ardından NUL gelir; `Value::text()` NUL'suz
  verir. `bembeyaz-kosync`'in `locate` modülü: `ProgressMapper`'ın tarayıcısıyla birebir aynı
  sonucu vermek için onun alanlarını (-1 yokluk değerleri, NUL ile doldurulmuş 12 baytlık etiket
  adları) aynen taşır. `bembeyaz-jpeg`'in API'si bu kurala uyar ama iç yapısı
  JPEGDEC'i işlev işlev izler ve onun bellek düzenini (okuma tamponundan Huffman tablolarına
  taşan okumalar, katsayıların üstüne yazılan pikseller) korur: bozuk dosyalarda bile çıktıyı
  oracle belirler. Bu düzeni değiştirmek bozuk dosyalarda çıktıyı değiştirebilir; böyle bir
  değişiklik kendi commit'inde yapılır.
- **Bulunan hatalar ayrı commit olur.** Her düzeltmeyle birlikte hatayı yakalayan
  bir test eklenir, ve düzeltme emülatör karşılaştırmasında kasıtlı fark olarak görünür.
- `cargo clippy --all-targets` (crate'lerde `clippy::pedantic`) uyarısız kalmalı.

- Firmware'e tek bir Rust staticlib bağlanır (`bembeyaz-ffi`). Yeni modül
  ayrı staticlib olmaz, `bembeyaz-ffi`'ye bağımlılık olarak eklenir.
- Rust tarafı C++ heap'ini kullanır (`device.rs`: `malloc`). 4 bayttan geniş hizalanması
  gereken blok, `align` bayt daha büyük bir `malloc` bloğundan kesilir; böylece her blok
  `malloc`'un seçtiği yere düşer: S3'te 4096 baytı aşan istek önce PSRAM'e gider
  (`lib/BlePageTurner/src/RadioPolicy.h`).
  Panic `abort()` çağırır; normal backtrace ve core dump üretir.
- Sınırda veri: dilim (`ptr` + `len`) girer; çıktı ya çağıranın tamponuna yazılır
  ya da `emit` geri çağrısıyla `std::string`'e (ya da bir `std::vector`'e) eklenir; Rust'ın
  tuttuğu bir değer, bir sonraki çağrıya kadar geçerli bir görünüm olarak da okunabilir (`bb_opf_field`;
  C++ hemen kopyalar). Rust, C++'ın
  `free` ile bırakacağı bellek döndürmez; döndürdüğü opak işaretçiyi C++ yine Rust'a (`*_free`) verir.
- Çağrılar arasında durum tutan bir Rust değeri ya C++'ın sağladığı opak bellekte durur
  (`bb_css_rules`), ya da Rust'ın heap'inde durur ve C++ ona işaretçiyle sahip olur
  (`bb_xml_new`/`bb_xml_free`). İlki, değer sabit boyutlu olup C++ nesnesinin içinde ya da belli
  bir bellekte durmalıysa; ikincisi, durum Rust'ın iç işiyse ve büyüyorsa. İkisinde de Rust'ın
  tuttuğu heap blokları Rust'ındır: değer bir kez `*_init` (Rust heap'inde `*_new`) ile kurulur,
  ömrü bitmeden `*_free` çağrılır, kopyalanmaz (C++ sınıfı kopyalanamaz ve taşınamaz). Heap tutmayan durum (`bb_css_tokenizer`)
  serbest bırakma gerektirmez; Rust tarafı bunu `needs_drop` ile derleme anında denetler.
- Güvenilmeyen girdiyi işleyen crate'ler (`bembeyaz-json`, `bembeyaz-toc`, `bembeyaz-zip`, `bembeyaz-inflate`, `bembeyaz-png`, `bembeyaz-jpeg`, `bembeyaz-bmp`, `bembeyaz-font`, `bembeyaz-xtc`, `bembeyaz-kosync`, `bembeyaz-library`, `bembeyaz-bidi`, `bembeyaz-css`, `bembeyaz-hyphenation`, `bembeyaz-xml`, `bembeyaz-opf`, `bembeyaz-path`, `bembeyaz-layout`, `bembeyaz-section`, `bembeyaz-book`, `bembeyaz-clippings`, `bembeyaz-io`) `clippy::indexing_slicing`,
  `unwrap_used` ve `panic`'i yasaklar; dosyadan okunan boyutla sınırsız bellek ayırmaz
  (ayırma sabit bir üst sınırla ya da zaten bellekte olan girdinin boyutuyla kısılır). Kesmeler ve tek bayt bozulmaları için panik
  olmadığını test eden bir mini-fuzz testi taşır.
- Büyük bir değer (ör. ~4,5 KB'lık inflate durumu) yerinde kurulmalı: Rust bunu
  garanti etmez, değer önce stack'te kurulup kopyalanabilir. `scripts/build_rust.py`
  her firmware build'inde (`pio run`) Rust fonksiyonlarının stack çerçevesini ölçer
  ve 2 KB'ı aşanı reddeder.
- Fonksiyon argümanı ve dönüş değeri olarak yalnızca tam sayı ve işaretçi geçiyor; `float`
  yalnızca bellekte, yerleşimi iki tarafta da denetlenen struct'ların içinde geçiyor
  (`CssStyle`). Değer olarak `float` geçmeden önce Xtensa'da float ABI uyumu doğrulanmalı.

## Taşıma sırası

Bağımlılıklara göre (önce yapraklar):

| Aşama | Modüller | Not |
| --- | --- | --- |
| 1 — saf metin | **Utf8 ✅** (küçük büyük harf eşlemesi `utf8SmallCapsUpper` da `bembeyaz-utf8`'te), AsciiText (`constexpr` iki satırlık yardımcı; FFI arkasında yavaşlar) ve HttpHeader (`std::string` alanlı birkaç satırlık ayar ayrıştırma/biçimleme; taşımak kazançtan çok yük getirir) C++'ta kalır, çağıranlarıyla birlikte taşınır; **XML ayrıştırıcı ✅** (expat'ın yerine; firmware `XmlParser.h`'yi, SDK'nın OPDS ve katalog okuyucuları `rust/include/expat.h`'deki expat API alt kümesini kullanır; metni expat'ın tamponunu taklit ederek aynı parçalarda verir, çünkü bölüm ayrıştırıcısı metni geri çağrı başına diziyor: o Rust'a geçince bu taklit kalkabilir), XmlParserUtils (yerel ad yardımcıları; aynı nedenle C++'ta kalır), **MiniBidi ✅** (minibidi davranışıyla birebir; UAX #9 düzeltmeleri sırada), **JSON ayrıştırıcı ✅** (SDK'nın StreamingJsonParser'ının yerine `bembeyaz-json`; `lib/JsonSax` SDK'nın API'sini taşır; SDK'nın OPDS 2, katalog ve LCP okuyucuları ile firmware'in LCP cihaz kaydı ve kilit açma ayrıştırıcıları değişmeden kullanır; fark testi SDK'nın C++ uygulamasıyla olay olay karşılaştırır; tampona sığmayan dizgi değeri SDK'nın aksine atılmaz, bir UTF-8 karakter sınırında kesilerek verilir; değeri bütün saklayan okuyucular (OAuth jetonu, eklenti kataloğu) böyle uzun bir jetonu ya da adresi kesik alır ve sonradan başarısız olur; fark testi bunu kapatıp karşılaştırır), ReleaseJsonParser (`lib/JsonParser`, güncelleme denetimi; C++'ta kalır, bu okuyucunun üstünde çalışır, aşama 5'te OTA ile taşınır) | Donanım yok, testleri var |
| 2 — sıkıştırma/veri | **ZipFile kapsayıcısı ✅**, **InflateStream ✅** (S3 ROM'undaki miniz 1.15 `tinfl_decompress`: ESP-IDF'in sunduğu kullanılır, flash harcamaz; emülatörde çözmesi kendi çözücümüzden ~1,4–1,9 kat hızlı, bölüm oluşturma süresi değişmedi; `bembeyaz-inflate` yalnızca PNG çözücülerinde), **InflateReader ✅** (silindi; kullanıcıları InflateStream'e geçti), Serialization (biçim okuyucuları Rust'a geçti; `Serialization.h` silindi, geriye kitap ve kütüphane kurulumundaki `BufferedFileWriter` ile `bembeyaz-io`'nun C++ uçları kaldı: `FileIo.h`, HalFile'ı ve BufferedFileWriter'ı `bb_io_file`/`bb_io_sink` olarak sunar) | Framebuffer ödünç alma ve inflate çağrısı C++'ta (`InflateStream`). ContentProtection (SDK) kendi C miniz kopyasını kullanıyor |
| 3 — EPUB çekirdeği | Epub: **kitap üst veri önbelleği ✅** (BookMetadataCache'in `book.bin`'i ve kurulduğu iki geçici dosyanın kayıtları, dosya başlığı ve üst veri okuması, LUT üzerinden spine ve içindekiler girdileri, kümülatif boyut pencereleri ve aramaları, `book.bin`'in kurulması (LUT'lar, her spine öğesinin içindekiler girdisi, kümülatif boyutlar) ve içindekiler href'inin spine öğesini bulma (karma dizini ya da dosyayı tarayarak) `bembeyaz-book`'un `cache` modülünde; biçimin sürümü de orada; dosyalar ve tamponları, bellek (dizin, pencere ve eşleme tabloları), ZIP'teki boyutlar (toplu arama dahil), 400 öğe eşiği, dizinin sıralanması, kilit ve NFC başlık birleştirme C++'ta; okuma ve yazma `bembeyaz-section` ile ortak `bembeyaz-io`'nun tamponlu okuyucu ve yazıcısıyla), **içindekiler okuyucuları ✅** (TocNcxParser ve TocNavParser'ın yerine `bembeyaz-toc`: XML'i `bembeyaz-xml`, yolları `bembeyaz-path` doğrudan okur; ikisi de `TocParser` katmanı üstünde ince sınıflar), **HTML varlık tablosu ✅** (`bembeyaz-html`; `lookupHtmlEntity`/`htmlEntityText` C++ çağıranlar için katman), **CSS ayrıştırıcı ✅** (önbellek biçimi aynı; dosya işlemleri C++'ta), **heceleme ✅** (hypher trie'leri crate'te; seçili dil C++'ta), **OPF ✅** (paket belgesi; manifest SD'deki geçici dosya yerine bellekte, 2 MiB ile sınırlı; yol çözümü `bembeyaz-path`, FsHelpers da onu kullanır; ayrıca `META-INF/container.xml` de `bembeyaz-opf`'in `container` modülünde, ContainerParser onun üstünde ince bir C++ katmanı), **ParsedText ✅** (paragraf yerleşimi: belirteçler, satır kırma, heceleme, iki yana yaslama, bidi, ruby; ölçümler GfxRenderer'dan geri çağrıyla; TextBlock, BlockStyle, bağlantı hedefleri ve satır içi yazı boyutu yuvaları C++'ta), **tablo sütun genişlikleri ✅** (ChapterHtmlSlimParser'ın tablo ön taraması ve CSS auto table genişlik paylaşımı `bembeyaz-layout`'un `table` modülünde; dosyayı okuma ve yazı genişliği geri çağrıyla C++'ta), Section (**bölüm dosyası tabloları ✅**: önbellekteki bölüm dosyasının başlığı ve yerleşim ayarı denetimi, yarım kalmış derlemenin ilerleme bilgisi, sayfa, çapa, paragraf, liste öğesi ve görünür metin konumu tablolarının okunması ve aranması ile derleme sonunda bu tabloların yazılıp dosyanın onaylanması `bembeyaz-section`'ın `file` modülünde; biçimin sürümü ve geçmişi de orada, sayfaları değiştiren bir C++ yerleşim değişikliği sürümü orada artırır; dosya açma, yeniden adlandırma, derlemenin yürütülmesi, derleme sırasında bellekteki tablo ve çapalarla yapılan aramalar (görünür konum taraması hariç) C++'ta; **bölüm sayfaları ✅**: sayfa gövdesinin (satırlar ve TextBlock arenası, ruby, satır stili, sözcük yazı tipleri; resim, yatay çizgi, büyük ilk harf, çerçeve kutusu; dipnotlar ve bağlantılar) okunup doğrulanması ve yazılması `bembeyaz-section`'ın `page` modülünde; nesneleri C++ geri çağrılarla (`bb_page_builder`) kurar, belleği (arena, ruby, sözcük yazı tipleri, resim yolları, dipnot ve bağlantı dizileri) C++ ayırır; çizim C++'ta) | En büyük parça (~19k satır); alt modül alt modül |
| 4 — görüntü/font | **PNG çözücüsü ✅** (bölüm resimleri, resim görüntüleyici, saydam uyku kaplaması, Ayrıntılar kapağı; PNGdec'in yerine `bembeyaz-png`; ölçekleme, titreme ve önbellek C++ dönüştürücüde kaldı), **JPEG çözücüsü ✅** (bölüm resimleri ve kapaklar; JPEGDEC'in yerine `bembeyaz-jpeg`, gri tonlu blok çıkışı; ölçekleme ve titreme C++ dönüştürücülerde kaldı), **PNG kapakları ✅** (PngToBmpConverter'ın kendi çözücüsünün yerine `bembeyaz-png`'nin `cover` okuyucusu; ölçekleme, titreme ve BMP yazımı C++'ta kaldı), **BMP okuyucusu ✅** (GfxRenderer'ın `Bitmap`'i: uyku ekranı, resim görüntüleyici, kapak küçük resimleri; başlık, palet ve satırların parlaklığa çevrilmesi `bembeyaz-bmp`'de; titreme ve 2 bitlik paketleme C++'ta kaldı; SleepActivity'nin saydam uyku kaplaması da başlığı `Bitmap` ile okur), EpdFont (**SD kart fontu başlığı ✅**: `SdCardFont::load`'un `.cpfont` başlığı ve stil içindekiler tablosu okuması, sayı sınırı denetimi, bölüm konumları, içerik özeti ve aralık tablosu denetimi `bembeyaz-font`'un `cpfont` modülünde; **SD kart fontu kerningi ✅**: sayfanın mini kerning matrisinin kurulması (sınıf tablolarının blok blok okunması, blok dizini, sınıfların yeniden numaralanması, matris satırlarının okunması) `bembeyaz-font`'un `kern` modülünde; **SD kart fontu glifleri ✅**: glif kayıtlarının çözülmesi (bitmap'i boyutundan kısa glif boş çizilir), sayfa önbelleğinin, ilerleme genişliği tablosunun ve anlık yüklemenin glif kaydı okuması (ardışık gliflerde konumlama atlanır) ve sayfa bitmap'lerinin tampona okunması `glyph` modülünde; dosya okuma, bellek ayırma ve tampon politikası, okuma sıralarının sıralanması, anlık yüklenen glifin bitmap okuması ve çizimdeki çift araması (`EpdFont::getKerning`) C++'ta; ligatür tablosu ayrıştırılmadan kopyalandığı için C++'ta kalır; **SD kart fontu aralık tablosu ✅**: `load`'un aralık tablosunu okuması, denetlemesi, 16 bitlik sıkışık biçime çevirmesi ve stillere paylaştırması ile kod noktasından glif araması `intervals` modülünde; tablonun belleği C++'ta; **SD kart fontu sayfa kümesi ✅**: sayfa metninden kod noktalarının toplanması (C++'ın verdiği bütçeyle, U+FFFD, ligatür çıktıları), önbellekteki sayfanın isteği karşılayıp karşılamadığının denetimi, önbellekle isteğin birleştirilmesi, kod noktalarının gliflere eşlenmesi ve mini aralıkların kurulması `page` modülünde; sıralama (C++'taki `std::sort` örneği zaten firmware'de), bellek ayırma ve tampon politikası C++'ta; **SD kart fontu ilerleme genişliği tablosu ✅**: düzen için sözcüklerden kod noktalarının toplanması (küçük büyük harflerin büyük biçimleri, boşluk, tire; C++'ın verdiği 4096 sınırıyla), tablodaki arama, getirilecek gliflerin planı (önbellekte olmayanlar, eksiklere U+FFFD) ve C++'ın verdiği 768 sınırıyla birleştirme `advance` modülünde; tablonun belleği ve büyütülmesi, sıralamalar C++'ta. `SdCardFont`'taki veri mantığından yalnızca anlık yüklenen glifin bitmap okuması ve ligatür kopyası C++'ta; gerisi bellek ve önbellek politikası, dosya erişimi ve Rust çağrıları. **Yerleşik sıkıştırılmış fontlar ✅**: `FontDecompressor`'ın sayfa glif kümesi, grupların bulunması, gliflerin açılmış gruptaki yerleri, satır dolgusunun atılması, sayfa tablosunun kurulması ve aranması `bembeyaz-font`'un `compressed` modülünde; bellek, inflate (ROM'daki tinfl), sayfa yuvaları ve anlık grup önbelleği C++'ta), GfxRenderer | Çerçeve tamponu sınırı |
| 5 — uygulama | LibraryIndex (**kütüphane metni ✅**: `LibraryText`'in katlama, harf grubu, kişi adı, yazar anahtarı, arama ve soyadı sıralama işlevleri `bembeyaz-library`'de; **dizin dosyası okuyucusu ✅**: `LibraryFormat.h`'in başlık doğrulaması ve bölüm yerleşimi ile `LibraryIndexFile`'ın sıralama, kayıt, ad, alan, klasör yolu ve son okunan araması `bembeyaz-library`'nin `format` modülünde, dosya tanıtıcısı ve `ClixHeader`/`ClixRecord` yapıları C++'ta; **dizin yazıcısı ✅**: `LibraryBuilder`'ın aşama dosyasından başlık, yazar anahtarı ve gösterilen yazımın soyadı sıralama anahtarlarını okuması, kişi başına tek yazım oylaması, değişiklik zamanlarının okunması ve kayıtların, iki permütasyonun ve ad bloblarının yazılması `bembeyaz-library`'nin `build` modülünde; klasör gezintisi, kitap üst verisinin okunması, aşama dosyası (düzeni `library.h`'te paylaşılır), eski dizinle eşleştirme, bellek ve bozulma kararları, `std::sort`, başlık ve klasör bölümü, dosyaların yerine konması C++'ta), Txt (**metin dönüştürücü ✅**: `TxtToHtml`'in TXT/Markdown gövdesini XHTML'e çevirmesi `bembeyaz-html`'in `text` modülünde; dosya okuma, arabellekler, XHTML başlığı ve önbellek sürüm etiketi C++'ta), Xtc (**XTC/XTCH ayrıştırıcısı ✅**: `XtcParser` başlığı, sayfa tablosunu, sayfa başlıklarını ve bölüm listesini `bembeyaz-xtc` ile okur; dosyayı açıp kapatmak C++'ta), KOReaderSync (**bölüm XPath çözücüsü ✅**: `ChapterXPathResolver` bölümün XHTML'ini `bembeyaz-kosync`'e akıtır; paragraf, görünür karakter konumu ve ilerleme için KOReader yolunu o bulur; ters yönde `ProgressMapper` gelen yolu `bembeyaz-kosync`'in `locate` modülüyle okur ve bölümde arar (XML ayrıştırıcısı değil, eski koddan gelen gevşek bir bayt tarayıcısı: CDATA'yı, yorumdaki ya da tırnaktaki `>`'yi gönderen yarıdan farklı sayabilir; ileride `bembeyaz-xml`'e geçebilir, iki yarıyı bir gidiş-dönüş testi bağlar); bölümü kitaptan okumak, Section önbelleği ve sayfa eşlemesi C++'ta), ClippingStore (**alıntı deposu ✅**: kitap başına alıntı dosyasının (başlık, kayıtlar ve metinler; 1–4. sürümler okunur, 4. sürüm yazılır) okunması ve yazılması ile silme günlüğündeki kimliklerin taranması `bembeyaz-clippings`'te; dosyaların açılması, yedek ve geçici dosya ile yeniden adlandırmalar, bellekteki alıntılar, silme günlüğüne yazma, kimlik üretme, kitap taşıma ve `ClippingStore.h`'teki vurgu için sözcük denetimi C++'ta), `src/` activity'leri | UI |
| 6 — platform | hal, freeink-sdk sürücüleri, `main` → Rust (esp-hal / esp-idf) | Arduino kalkar |
