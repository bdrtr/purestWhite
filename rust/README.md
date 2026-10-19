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
  Sınırın C biçimini (C string, `int` dönüş kodları) FFI katmanı ve shim taşır. Tek istisna
  `bembeyaz-layout`: renderer ve `TextBlock` C string aldığı için sözcükleri NUL ile biten
  biçimde tutar, ölçer ve verir. `bembeyaz-jpeg`'in API'si bu kurala uyar ama iç yapısı
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
- Güvenilmeyen girdiyi işleyen crate'ler (`bembeyaz-zip`, `bembeyaz-inflate`, `bembeyaz-png`, `bembeyaz-jpeg`, `bembeyaz-bidi`, `bembeyaz-css`, `bembeyaz-hyphenation`, `bembeyaz-xml`, `bembeyaz-opf`, `bembeyaz-path`, `bembeyaz-layout`) `clippy::indexing_slicing`,
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
| 1 — saf metin | **Utf8 ✅**, AsciiText, HttpHeader, **XML ayrıştırıcı ✅** (expat'ın yerine; firmware `XmlParser.h`'yi, SDK'nın OPDS ve katalog okuyucuları `rust/include/expat.h`'deki expat API alt kümesini kullanır; metni expat'ın tamponunu taklit ederek aynı parçalarda verir, çünkü bölüm ayrıştırıcısı metni geri çağrı başına diziyor: o Rust'a geçince bu taklit kalkabilir), XmlParserUtils (yerel ad yardımcıları), **MiniBidi ✅** (minibidi davranışıyla birebir; UAX #9 düzeltmeleri sırada), JsonParser | Donanım yok, testleri var |
| 2 — sıkıştırma/veri | **ZipFile kapsayıcısı ✅**, **InflateStream ✅** (S3 ROM'undaki miniz 1.15 `tinfl_decompress`: ESP-IDF'in sunduğu kullanılır, flash harcamaz; emülatörde çözmesi kendi çözücümüzden ~1,4–1,9 kat hızlı, bölüm oluşturma süresi değişmedi; `bembeyaz-inflate` yalnızca PNG çözücülerinde), **InflateReader ✅** (silindi; kullanıcıları InflateStream'e geçti), Serialization | Framebuffer ödünç alma ve inflate çağrısı C++'ta (`InflateStream`). ContentProtection (SDK) kendi C miniz kopyasını kullanıyor |
| 3 — EPUB çekirdeği | Epub: **CSS ayrıştırıcı ✅** (önbellek biçimi aynı; dosya işlemleri C++'ta), **heceleme ✅** (hypher trie'leri crate'te; seçili dil C++'ta), **OPF ✅** (paket belgesi; manifest SD'deki geçici dosya yerine bellekte, 2 MiB ile sınırlı; yol çözümü `bembeyaz-path`, FsHelpers da onu kullanır), **ParsedText ✅** (paragraf yerleşimi: belirteçler, satır kırma, heceleme, iki yana yaslama, bidi, ruby; ölçümler GfxRenderer'dan geri çağrıyla; TextBlock, BlockStyle, bağlantı hedefleri ve satır içi yazı boyutu yuvaları C++'ta), Section | En büyük parça (~19k satır); alt modül alt modül |
| 4 — görüntü/font | **PNG çözücüsü ✅** (bölüm resimleri, resim görüntüleyici, saydam uyku kaplaması, Ayrıntılar kapağı; PNGdec'in yerine `bembeyaz-png`; ölçekleme, titreme ve önbellek C++ dönüştürücüde kaldı), **JPEG çözücüsü ✅** (bölüm resimleri ve kapaklar; JPEGDEC'in yerine `bembeyaz-jpeg`, gri tonlu blok çıkışı; ölçekleme ve titreme C++ dönüştürücülerde kaldı), **PNG kapakları ✅** (PngToBmpConverter'ın kendi çözücüsünün yerine `bembeyaz-png`'nin `cover` okuyucusu; ölçekleme, titreme ve BMP yazımı C++'ta kaldı), EpdFont, GfxRenderer | Çerçeve tamponu sınırı |
| 5 — uygulama | LibraryIndex, Txt, Xtc, KOReaderSync, `src/` activity'leri | UI |
| 6 — platform | hal, freeink-sdk sürücüleri, `main` → Rust (esp-hal / esp-idf) | Arduino kalkar |
