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

# Inflate: PNG kapak (zlib akışı), stored ve level-9 bölümler
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
# iç DTD ve 32 KB'tan uzun bir paragraf. Bozuk ikizi açılışta aynı hataları vermeli.
python3 emu/make_xml_epub.py emu/books/xml
emu/compare.sh emu/xml.txt x4pro emu/books/xml/xml-test.epub
emu/compare.sh emu/open-book.txt x4pro emu/books/xml/broken/xml-broken.epub

# Bozuk EPUB'lar (her biri kendi kartında):
python3 emu/make_bad_epubs.py emu/books/bad
emu/compare.sh emu/open-book.txt x4pro emu/books/bad/max-size-cover/max-size-cover.epub
```

Davranışı korumak için yapılan taşımada beklenen sonuç "N of N screenshots
identical". Kasıtlı bir düzeltme fark üretiyorsa, bu fark commit mesajında
açıklanır.

## Kurallar

- **Her şeyi kendimiz yazarız.** Firmware'e bağlanan Rust kodunda (`bembeyaz-ffi`'nin
  bağımlılıkları) üçüncü taraf crate yoktur: algoritmalar standarttan (UAX #9,
  RFC 1951 …) kendimiz yazılır, en küçük ve en hızlı olacak şekilde. Testlerde
  referans olarak kullanılan crate'ler (`zip`; test verisini sıkıştıran ve
  karşılaştırma testinde `bembeyaz-inflate`'in yerini aldığı çözücü olan miniz_oxide)
  de zamanla kendi ürettiğimiz fixture'larla değiştirilir.
- **Taşıma önce eski davranışı korur.** Eski koda karşı yazılmış oracle testleri
  değişmeden geçer; standarttan sapmalar sonra, her biri kendi commit'inde düzeltilir.
- **Rust tarafı birebir çeviri değildir.** Crate'ler dilim, iterator, `Option` ve
  `Result` kullanır; NUL ile biten string, -1 gibi özel değerler ve sessiz hata yoktur.
  Sınırın C biçimini (C string, `int` dönüş kodları) FFI katmanı ve shim taşır.
- **Bulunan hatalar ayrı commit olur.** Her düzeltmeyle birlikte hatayı yakalayan
  bir test eklenir, ve düzeltme emülatör karşılaştırmasında kasıtlı fark olarak görünür.
- `cargo clippy --all-targets` (crate'lerde `clippy::pedantic`) uyarısız kalmalı.

- Firmware'e tek bir Rust staticlib bağlanır (`bembeyaz-ffi`). Yeni modül
  ayrı staticlib olmaz, `bembeyaz-ffi`'ye bağımlılık olarak eklenir.
- Rust tarafı C++ heap'ini kullanır (`device.rs`: `malloc`/`heap_caps_aligned_alloc`).
  Panic `abort()` çağırır; normal backtrace ve core dump üretir.
- Sınırda veri: dilim (`ptr` + `len`) girer; çıktı ya çağıranın tamponuna yazılır
  ya da `emit` geri çağrısıyla `std::string`'e (ya da bir `std::vector`'e) eklenir. Rust, C++'ın
  `free` ile bırakacağı bellek döndürmez; döndürdüğü opak işaretçiyi C++ yine Rust'a (`*_free`) verir.
- Çağrılar arasında durum tutan bir Rust değeri ya C++'ın sağladığı opak bellekte durur
  (`bb_css_rules`), ya da Rust'ın heap'inde durur ve C++ ona işaretçiyle sahip olur
  (`bb_xml_new`/`bb_xml_free`). İlki, değer sabit boyutlu olup C++ nesnesinin içinde ya da belli
  bir bellekte durmalıysa; ikincisi, durum Rust'ın iç işiyse ve büyüyorsa. İkisinde de Rust'ın
  tuttuğu heap blokları Rust'ındır: değer bir kez `*_init` (Rust heap'inde `*_new`) ile kurulur,
  ömrü bitmeden `*_free` çağrılır, kopyalanmaz (C++ sınıfı kopyalanamaz ve taşınamaz). Heap tutmayan durum (`bb_inflate`, `bb_css_tokenizer`)
  serbest bırakma gerektirmez; Rust tarafı bunu `needs_drop` ile derleme anında denetler.
- Güvenilmeyen girdiyi işleyen crate'ler (`bembeyaz-zip`, `bembeyaz-inflate`, `bembeyaz-bidi`, `bembeyaz-css`, `bembeyaz-hyphenation`, `bembeyaz-xml`) `clippy::indexing_slicing`,
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
| 2 — sıkıştırma/veri | **ZipFile kapsayıcısı ✅**, **InflateStream ✅** (kendi çözücümüz), **InflateReader ✅** (silindi; kullanıcıları InflateStream'e geçti), Serialization | Framebuffer ödünç alma C++'ta kaldı; Rust belleği çağırandan alır. ContentProtection (SDK) kendi C miniz kopyasını kullanıyor |
| 3 — EPUB çekirdeği | Epub: **CSS ayrıştırıcı ✅** (önbellek biçimi aynı; dosya işlemleri C++'ta), **heceleme ✅** (hypher trie'leri crate'te; seçili dil C++'ta), OPF, ParsedText, Section | En büyük parça (~19k satır); alt modül alt modül |
| 4 — görüntü/font | Png/JpegToBmp, EpdFont, GfxRenderer | Çerçeve tamponu sınırı |
| 5 — uygulama | LibraryIndex, Txt, Xtc, KOReaderSync, `src/` activity'leri | UI |
| 6 — platform | hal, freeink-sdk sürücüleri, `main` → Rust (esp-hal / esp-idf) | Arduino kalkar |
