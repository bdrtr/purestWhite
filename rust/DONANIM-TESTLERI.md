# Bekleyen donanım testleri

Emülatör ve host testlerinden geçen ama X4 Pro'da henüz denenmemiş değişiklikler. Her
yeni commit kendi maddesini buraya ekler. Hepsi tek bir yüklemede, en sondaki commit ile
denenir; denenen madde silinir. Hata bulunursa hangi maddede çıktığını yazın, o commit'e
dönülür.

```sh
pio run -e x4pro -t upload && pio device monitor   # günlük için seri port açık kalsın
```

Karşılaştırma için önceki sürüm gerekiyorsa: `../crosspoint-base` değiştirilmemiş
CrossPoint, `../crosspoint-prev` 609d473.

## İlk açılış

- [ ] **Kitap önbelleği yeniden kuruluyor** (5f321eb, `BOOK_CACHE_VERSION` 11). Daha önce
  açılmış bir kitabı açın: günlükte bir kez `Cache version mismatch: expected 11, got 10`,
  ardından indeksleme görünmeli. Kitabı kapatıp yeniden açın: indeksleme olmamalı.
  Okuma konumu korunmalı. Her kitap ilk açılışta bir kez yeniden sayfalanır, bu normal.

## XML ayrıştırıcı (3cfabb3, 5f321eb)

- [ ] Farklı kaynaklardan birkaç kitap açın (EPUB 2 ve 3, Calibre çıkışı, yayınevi
  kitabı); sayfalar ve içindekiler eskisi gibi olmalı.
- [ ] `<!DOCTYPE html>` kullanan bir EPUB 3 kitap (çoğu yeni kitap): bölümler açılmalı,
  `&nbsp;`, `&mdash;` gibi işaretler doğru görünmeli, içindekiler tam olmalı (eskiden
  "Failed to index - invalid book" ya da eksik içindekiler).
- [ ] OPDS kataloğu: göz atma, arama, indirme.
- [ ] WebDAV ile dosya listesi.
- [x] KOReader eşitleme: konum gönderme ve alma.
- [ ] KOReader eşitleme, `&nbsp;` gibi HTML varlıkları bol bir kitapta ("fix: KOReader
  sync counts HTML entities as the chapter shows them"): bölüm ortasında konum gönderin,
  KOReader aynı yeri açmalı; KOReader'dan gelen konum da aynı sayfaya düşmeli. Eskiden
  DTD'siz bölümlerde konum hiç gönderilemiyordu, dış DTD'li bölümlerde her varlık konumu
  bir karakter kaydırıyordu.
- [ ] Çok uzun bir bölüm (yüzlerce sayfa) ve büyük tablolu bir kitap: açılırken ve
  okurken yeniden başlatma ya da donma olmamalı (ayrıştırıcının yığın kullanımı arttı).

## Yol çözümü ("rust: path helpers")

- [ ] Bölüm, resim ya da bağlantı dosyalarının adında boşluk, `%20` ya da `../` geçen bir
  kitap: bölümler açılmalı, resimler görünmeli, içindekiler ve dipnot bağlantıları çalışmalı.
- [ ] WebDAV ve web sunucusu: bir alt klasöre dosya yükleyin, listeleyin, indirin.

## OPF (paket belgesi) ("rust: our own package document reader")

- [ ] Kütüphanede birkaç kitap: başlık, yazar ve seri adı/sırası eskisi gibi görünmeli
  (Calibre serisi ve EPUB 3 koleksiyonu olan birer kitap iyi olur).
- [ ] Kapak resimleri: EPUB 2 (`meta name="cover"`) ve EPUB 3 (`cover-image`) kapaklı
  birer kitap; kapaklar görünmeli (ana ekran kapak ızgarası da).
- [ ] Çok bölümlü büyük bir kitap (yüzlerce manifest öğesi): ilk açılış eskisinden yavaş
  olmamalı, bölüm sırası ve içindekiler doğru olmalı. Günlükte `OPF pass completed in`
  süresini not edin.
- [ ] Binlerce manifest öğesi olan bir kitap (antoloji, sözlük ya da çizgi roman EPUB'ı):
  açılmalı; `Parse error` görülmemeli.

## Paragraf yerleşimi ("rust: our own paragraph layout")

- [ ] Farklı hizalamalarla birkaç kitap (iki yana yaslı, sola yaslı, ortalı): satır sonları ve
  sözcük aralıkları eskisi gibi olmalı; sayfa sayısı (alt bilgideki 1/N) değişmemeli.
- [ ] Heceleme açıkken Türkçe ve Almanca birer kitap: tireler aynı yerlerde olmalı.
- [ ] Odaklanmış Okuma (Focus Reading) açıkken: sözcük başları kalın olmalı, hizalama kaymamalı.
- [ ] Japonca/Çince bir kitap (ruby varsa iyi olur), Korece bir kitap ve Arapça/İbranice bir
  kitap: satır kırma ve sağdan sola sıralama eskisi gibi olmalı.
- [x] Dipnot bağlantıları olan bir kitap: bağlantıya dokunulunca dipnot açılmalı.
- [ ] SD karttan yüklenen bir yazı tipiyle uzun bir bölüm: ilk sayfa oluşturma süresi
  (`Time to parse and build pages`) eskisinden uzun olmamalı.
- [ ] Çok uzun tek paragraflı bir bölüm (ör. Çince romanda bölüm boyu tek paragraf): açılmalı,
  yeniden başlatma olmamalı.

## DEFLATE ve yazı tipleri (b340e76, fdd0780)

- [ ] Kitap açma ve bölüm oluşturma süresi; önceki sürümle kabaca karşılaştırın (günlükte
  `Time to parse and build pages`). EPUB dosyaları, yazı tipleri ve sözlük artık ROM'daki
  inflate ile açılıyor: aşağıdaki "ROM'daki inflate" bölümü.
- [ ] SD karttan yüklenen bir yazı tipiyle okuma: glifler bozulmadan çizilmeli, sayfa
  çevirme hızı eskisi gibi olmalı.
- [ ] PNG kapaklı bir kitap: kapak görünmeli.
- [x] Sözlük (.dict.dz): kelime aramaları tanımları getirmeli.

## CSS (46ebf6e)

- [ ] Stil yoğun bir kitap (başlıklar, girintiler, hizalama, kalın/italik sınıfları):
  görünüm eskisi gibi olmalı. CSS önbelleği biçimi değişmedi, yeniden kurulmamalı.

## Heceleme (609d473)

- [ ] Heceleme açıkken Türkçe ve İngilizce bir kitap: satır sonları tireyle bölünmeli,
  sayfa düzeni eskisi gibi olmalı.

## Çift yönlü metin (1b556ee, 3987490, 09112bb)

- [ ] Arapça ya da İbranice yazı tipi yüklü bir kitap: kelime sırası, parantez yönleri,
  sayılar ve Arapça harf birleşmeleri doğru olmalı.

## BLE kumandalar (598e41e, 76620d4)

- [ ] Dört kumanda eşleyin; dördü de yeniden başlatmadan sonra bağlanabilmeli
  (eskiden ilk kumanda anahtarını kaybediyordu).
- [ ] Dört kumanda eşliyken beşinciyi tarayın: listede pasif, "Forget a paired remote
  first" (arayüz dilinde karşılığı yoksa İngilizce) yazmalı. Birini unutun: beşinci
  eşlenebilmeli, diğer üçü çalışmaya devam etmeli.

## Sıkıştırma ("fix: inflate refuses a distance before the start of the stream")

Bu iki düzeltme artık yalnızca PNG resimlerinde (`bembeyaz-png`) geçerli; elle bozulmuş
dosyalarda çıkar ve host testleri kapsar. Bozuk EPUB denemesi aşağıdaki "ROM'daki inflate"
bölümünde.

- [x] Cihazda ayrıca denenecek bir şey yok: "PNG resimleri" bölümündeki `png-broken.epub`
  denemesinde donma olmaması yeterli. Sağlam kitaplar bu değişikliklerden etkilenmez.

## Sıkıştırma hızı ("rust: inflate for the PNG decoder")

- [ ] Büyük bir kitabın önbelleğini silin (kitap açıkken okuma menüsünde Delete Book Cache)
  ve açın: sayfalar eskisi gibi görünmeli; günlükteki `Time to parse and build pages` süresi
  önceki sürümden uzun olmamalı (emülatörde fark 3 ms içinde). Kapak küçük resimleri (PNG
  kapaklar) de görünmeli.

## PNG resimleri ("rust: our own PNG decoder for chapter images")

Bölüm resimleri ilk çözümden sonra önbelleğe alınır: aşağıdakileri denemeden önce kitabın
önbelleğini silin (kitap açıkken okuma menüsünde Delete Book Cache) ya da kitabı ilk kez
açın. Hata belirtileri: kayık ya da çizgili satırlar, yanlış gri tonlar, saydam yerlerin siyah
çıkması, boş resim, donma ya da yeniden başlatma.

- [ ] PNG resimli birkaç kitap (diyagram, ekran görüntüsü, saydam logo): resimler eskisi gibi
  görünmeli. Gri tonlar, saydam bölgeler, küçültülmüş büyük resimler de aynı olmalı.
- [ ] `python3 emu/make_png_epub.py emu/books/png` ile üretilen `png.epub`'ı karta kopyalayıp
  13 bölümü sırayla açın; her bölümde tek resim var. Görünüşleri
  `emu/compare.sh emu/png.txt x4pro emu/books/png/png.epub` çıktısındaki
  `emu/runs/png-png-*/report.html` ekran görüntüleriyle aynı olmalı.
- [x] Aynı klasördeki `png-broken.epub`: bölüm açılmalı, cihaz donmamalı. Günlükte
  `Decode failed: 1` görünmeli, resmin yerinde boş bir çerçeve olmalı (emülatörde de böyle).
  Eski sürüm bu resimde sonsuza dek bekliyordu.
- [ ] Hız: büyük bir PNG resmi olan bir bölümü önbelleği silinmiş olarak açın, günlükteki
  `PNG decoding complete - render time:` süresini önceki sürümle karşılaştırın. Emülatörde
  1600×1000 RGB bir resim için ~1195 ms, eskisinde ~1135 ms (%5 yavaş). Emülatör PSRAM'i
  gerçekte olduğundan hızlı modelliyor; cihazda fark %10'u aşarsa not edin.
- [ ] Dosya gezgininden bir `.png` dosyası açın (büyük bir resim de deneyin): resim eskisi gibi
  görünmeli, önceki/sonraki resme geçiş çalışmalı.
- [ ] OPDS kataloğunda PNG kapaklı bir kitabın Details (Ayrıntılar) ekranı: kapak görünmeli (bu ekran
  kapağı bölüm resmi çözücüsüyle çizer).
- [x] Uyku ekranı ayarını **Transparent** yapın, kartın köküne alfa kanallı bir
  `sleep-overlay.png` koyun (kökte `sleep-overlay.bmp` olmasın, o önce gelir), cihazı uyutun:
  saydam yerlerden alttaki ekran, opak yerlerde resim görünmeli.
- [x] BLE kumanda bağlıyken PNG'li bir bölümü ilk kez açın: kumanda bağlı kalmalı, sayfa
  çevirme çalışmalı (çözücü dahili RAM'den hiçbir zaman 4 KB'tan büyük blok istemez).

## JPEG resimleri ("rust: our own JPEG decoder")

Resimler ve kapaklar ilk çözümden sonra önbelleğe alınır: aşağıdakileri denemeden önce kitabın
önbelleğini silin (kitap açıkken okuma menüsünde Delete Book Cache) ya da kitabı ilk kez açın.
Hata belirtileri: kayık ya da bloklu (8×8 kareli) görüntü, yanlış gri tonlar, eksik şeritler,
boş resim, donma ya da yeniden başlatma.

- [ ] JPEG resimli birkaç kitap (fotoğraf, taranmış sayfa, çizgi roman): resimler eskisi gibi
  görünmeli; büyük fotoğraflar da (sayfaya küçültülerek).
- [ ] `python3 emu/make_jpeg_epub.py emu/books/jpeg` ile üretilen `jpeg.epub`'ı karta kopyalayıp
  11 bölümü sırayla açın; görünüşleri
  `emu/compare.sh emu/jpeg.txt x4pro emu/books/jpeg/jpeg.epub` çıktısındaki
  `emu/runs/jpeg-jpeg-*/report.html` ekran görüntüleriyle aynı olmalı. İlerlemeli JPEG'ler
  bulanık görünür (eskiden de öyleydi: yalnızca ilk tarama, 1/8 boyutta çözülür). CMYK bölümü
  bozuk, şeritli görünür (eskiden de öyleydi).
- [ ] JPEG kapaklı kitaplar: Home'daki küçük resimler ve uyku ekranındaki kapak görünmeli
  (`jpeg.epub`'ın kapağı da); OPDS kataloğunda JPEG kapaklı bir kitabın Details (Ayrıntılar)
  ekranında da kapak görünmeli.
- [ ] Dosya gezgininden bir `.jpg` dosyası açın (büyük bir fotoğraf da deneyin): resim eskisi
  gibi görünmeli, önceki/sonraki resme geçiş çalışmalı.
- [ ] Resim görüntüleyicide bir `.jpg` açıkken **Kapak Ayarla** (Set Cover) düğmesine basın,
  sonra cihazı uyutun: uyku ekranında resim eskisi gibi görünmeli.
- [ ] Hız: `jpeg.epub`'ın büyük resimli üç bölümünü önbelleği silinmiş olarak açın, günlükteki
  `JPEG decoding complete - render time:` sürelerini önceki sürümle karşılaştırın. Emülatörde
  yeni çözücü daha yavaş: "Large, at 1/2" ~425 ms (eskisi ~383, %11), "Larger, at 1/4" ~742 ms
  (eskisi ~656, %13), "Wide, at 1/8" ~470 ms (eskisi ~410, %15); küçük resimlerde birkaç ms.
  Kapak küçük resminin süresi, günlükte `Converting JPEG to 1-bit BMP` ile
  `Successfully converted JPEG to BMP` satırlarının zaman damgaları arasıdır: emülatörde ~232 ms
  (eskisi ~219, %6). Cihazda fark %30'u aşarsa ya da kapak/sayfa açılışı gözle fark edilir
  biçimde yavaşlarsa not edin.

## Bozuk JPEG ("fix: JPEG open no longer loops on a segment past the end of the file")

- [x] `python3 emu/make_jpeg_epub.py emu/books/jpeg` ile üretilen `jpeg-broken.epub`'ı karta
  kopyalayıp bölümünü açın: sayfa açılmalı, cihaz donmamalı. Sayfada yalnızca başlık görünür,
  resim atlanır (günlükte `Failed to open JPEG for dimensions (err=2)`); emülatörde de böyle. Eski
  sürüm bu resimde sonsuza dek bekliyordu. Sağlam JPEG'ler bu değişiklikten etkilenmez.

## Hizalı Rust blokları ("fix: aligned Rust allocations follow malloc's PSRAM rule")

- [ ] Görünür bir değişiklik yok: 4 bayttan geniş hizalanan Rust blokları (XML ayrıştırıcıları, OPF
  okuyucu) artık bir `malloc` bloğundan kesiliyor. Önbelleği silinmiş birkaç kitabı açıp bölümler
  arasında gezinin, OPDS kataloğuna da göz atın: günlükte `Rust panic`, yeniden başlatma ya da donma
  olmamalı.
- [x] BLE kumanda bağlıyken bir kitabı ilk kez açın: kumanda bağlı kalmalı, sayfa çevirme çalışmalı.

## PNG kapak bit derinliği ("fix: PNG covers refuse a bit depth the color type does not allow")

- [ ] Bu durum yalnızca elle bozulmuş dosyalarda çıkar ve host testleri kapsar: geçersiz bit
  derinlikli bir PNG kapak artık cihazı çökertmez, kapak gösterilmez. PNG kapakların cihaz denemesi
  aşağıdaki "PNG kapakları" bölümünde.

## PNG kapakları ("rust: PNG covers through bembeyaz-png")

Kapak küçük resimleri ve uyku ekranı kapağı ilk üretimden sonra önbellekte kalır: aşağıdakileri
denemeden önce kitabın önbelleğini silin (kitap açıkken okuma menüsünde Delete Book Cache / Kitap
Önbelleğini Sil) ya da kitabı ilk kez açın. Hata belirtileri: boş, kayık ya da çizgili kapak, yanlış
gri tonlar, donma ya da yeniden başlatma.

- [ ] PNG kapaklı birkaç kitap (paletli, gri, renkli ve saydam kapaklar): Home'daki küçük resimler
  ve uyku ekranındaki kapak eskisi gibi görünmeli (Uyku Ekranı / Sleep Screen ayarı **Kapak** /
  Cover: uyku ekranı son açılan kitabın kapağını gösterir).
- [ ] `python3 emu/make_png_epub.py emu/books/png` ile üretilen `png.epub`'ı (paletli kapak) ve
  `python3 emu/make_inflate_epub.py emu/books/inflate` ile üretilen `inflate.epub`'ı (RGB kapak)
  karta kopyalayıp açın, Home'a dönün: aynı temada küçük resimler
  `emu/compare.sh emu/png.txt x4pro emu/books/png/png.epub` ve
  `emu/compare.sh emu/inflate.txt x4pro emu/books/inflate/inflate.epub` çıktılarındaki
  (`emu/runs/png-png-*/report.html`, `emu/runs/inflate-inflate-*/report.html`) `home-cover` ekran
  görüntüleriyle aynı olmalı.
- [ ] Hız: `png.epub`'ın küçük resmi üretilirken günlükteki `Converting PNG to 1-bit BMP` ile
  `Successfully converted PNG to BMP` satırlarının zaman damgaları arasındaki süreyi ölçün.
  Emülatörde ~400 ms (değiştirilmemiş CrossPoint'te ~495), `inflate.epub`'ın RGB kapağında ~450 ms
  (CrossPoint ~490). Cihazda `../crosspoint-base` build'inden %10'dan fazla uzunsa not edin.
- [ ] Bir `.txt` kitabın yanına aynı adda bir `.png` koyun (aynı adda `.bmp`, `.jpg` ya da `.jpeg`
  varsa o önce gelir; aynı adda resim yoksa klasördeki `cover.png` da kullanılır): kitabı açıp
  Home'a dönün, küçük resim ve uyku ekranı kapağı görünmeli.
- [ ] Eşlenmiş ama henüz bağlanmamış bir BLE kumanda varken önbelleği silinmiş PNG kapaklı bir
  kitabın küçük resmini yeniden üretin (kitabı açıp kapatın, cihazı yeniden başlatıp Home'a
  dönün): günlükteki `HID begin skipped` / `BLE start deferred` satırlarını ve boş/en büyük blok
  rakamlarını not edin; kumanda bağlanmalı (gerekirse bir sonraki kitap açılışında), sayfa çevirme
  çalışmalı. Kapak okunurken ~2,3 KB'lık okuyucu durumu (eskiden görev yığınındaydı) dahili heap'te
  durur; okuyucu dahili RAM'den hiçbir zaman 4 KB'tan büyük blok istemez (büyük satır tamponları
  PSRAM'e gider).

## ROM'daki inflate ("inflate: InflateStream decodes with the ESP32-S3 ROM's miniz")

EPUB içindeki dosyalar (bölümler, OPF, içindekiler, CSS, resim dosyaları), sıkıştırılmış yazı
tiplerinin glif grupları (yerleşik okuma yazı tipleri ve indirilen `.cpfont`'lar), KOReader
eşitlemesinin okuduğu bölümler ve sözlük (.dict.dz) tanımları artık S3 ROM'undaki miniz ile
açılıyor (önceden kendi Rust çözücümüzle). PNG resimlerin içi hâlâ `bembeyaz-png` ile çözülür.
Hata belirtileri: kitabın açılmaması, eksik ya da bozuk sayfa, bozuk ya da eksik glifler, sözlükte
tanımın gelmemesi, donma ya da yeniden başlatma; sağlam bir dosyada günlükte `Inflate failed`.

- [ ] Birkaç kitabın önbelleğini silip (kitap açıkken okuma menüsünde Delete Book Cache / Kitap
  Önbelleğini Sil) açın, bölümler arasında gezinin: sayfalar, içindekiler, stiller ve resimler
  eskisi gibi olmalı. Günlükteki `Time to parse and build pages` süresi önceki sürümden uzun
  olmamalı (emülatörde aynı kaldı).
- [ ] `python3 emu/make_inflate_epub.py emu/books/inflate` ile üretilen `inflate.epub`'ı karta
  kopyalayıp açın: stored bölüm ve ~150 KB'lık level-9 bölüm,
  `emu/compare.sh emu/inflate.txt x4pro emu/books/inflate/inflate.epub` çıktısındaki
  (`emu/runs/inflate-inflate-*/report.html`) ekran görüntüleriyle aynı görünmeli.
- [ ] Yerleşik yazı tipleriyle (farklı boyutlar, kalın/italik) ve indirilmiş bir yazı tipiyle
  okuma: glifler bozulmadan çizilmeli, sayfa çevirme eskisi kadar hızlı olmalı.
- [x] Sözlük (.dict.dz): birkaç kelime, uzun tanımlı bir kelime de arayın; tanımlar tam gelmeli.
- [x] KOReader eşitleme: konum gönderme ve alma çalışmalı.
- [ ] Bozuk bir EPUB: bir kitabın kopyasında dosyanın ortasındaki birkaç baytı bir hex
  düzenleyiciyle değiştirin (dosyanın sonundaki ZIP dizini sağlam kalsın; yarıda kesilmiş bir
  dosya inflate'e hiç ulaşmaz) ve bölümleri açın: donma ya da yeniden başlatma olmamalı; bozuk
  bölüm ya hata vermeli (günlükte `Inflate failed: ...`) ya da bozuk metin göstermeli. ROM'un
  miniz'i bazı bozuk verileri kendi çözücümüzden daha gevşek karşılar (ör. akışın başından
  önceye uzanan bir mesafe 0 okunur).
- [ ] BLE kumanda bağlıyken önbelleği silinmiş bir kitabı açın ve sözlükte bir kelime arayın:
  kumanda bağlı kalmalı, sayfa çevirme çalışmalı (çözücü durumu ~4,5 KB'tan ~11 KB'a çıktı;
  ödünç framebuffer'da ya da PSRAM'de durur).

## Sunucu sertifikası doğrulaması ("net: SecureNet moves into the fork", "net: HTTPS checks server certificates")

HTTPS artık sunucuyu doğruluyor: zincir ESP-IDF'in kök paketindeki (150 kök) bir köke varmalı,
sertifika sunucunun adını taşımalı ve tarihleri geçerli olmalı. Ayar: Settings → System →
Verify Server Certificates (Sunucu sertifikalarını doğrula), varsayılan açık. CrossPoint
(crosspointreader.com), FreeInk (freeink.org) ve GitHub adresleri ayardan bağımsız hep
doğrulanır. SDK'nın OPDS OAuth girişi de ayara uyar. wolfSSL'e SHA-384/512 ve hızlı P-384
eklendi; P-384 imzalı zincirler (Let's Encrypt, Google, Sectigo) bunlarsız doğrulanamıyordu.

Yeni bağlantı başına ek süre (emülatördeki komut sayısından 240 MHz için hesaplanan; cihazda biraz
daha uzun olabilir): Let's Encrypt ECDSA zinciri (sync.crosspointreader.com) ≈1,6 s, SSL.com
zinciri (ör. example.com) ≈1,2 s, github.com ≈1,0 s, Google Trust Services zinciri ≈0,7 s. KOSync
eşitleme başına bir bağlantı açar (bkz. "KOSync: eşitleme başına tek bağlantı"); varsayılan sunucuyla
bir eşitleme ≈1,6 s uzar, sonuç ekranında ≈1 dakikadan uzun beklenirse gönderme için bir kez daha.
Oturum bileti gönderen ve oturum sürdüren sunucularda açılıştan sonraki ikinci bağlantıdan
itibaren zincir yeniden denetlenmez (bkz. "TLS oturum sürdürme"). sync.crosspointreader.com ve
github.com oturum sürdürmüyor (OpenSSL istemcisiyle de denendi; sunucu tarafında); bu sunucularda
ek süre her bağlantıda kalır.

Günlükteki `[SecureClient] certificate N refused: K` satırında N zincirdeki sıra (0 sunucunun
kendi sertifikası), K wolfSSL kodu:
- `-188` imzalayan bulunamadı: ara sertifika eksik ya da kök pakette yok. En üstteki sertifikanın
  tarihi geçersizse ya da imzası tutmazsa da -188 çıkar. İmza hatasında üstünde
  `esp-x509-crt-bundle: Certificate matched but signature verification failed` görünür; kök
  pakette yoksa yalnız `Failed to verify certificate`. Önce saati denetleyin.
- `-150` / `-151` henüz geçerli değil / süresi dolmuş (saat yanlışsa geçerli sunucuda da çıkar).
- `-322` sertifika sunucunun adını taşımıyor; `-155` imza bozuk; `-232` hash türü derlenmemiş.
- `certificate N not checked: low memory` bellek yetmedi demektir, sertifika reddi değil. Bellek
  hataları sayıyla yazılır: `wolfSSL_connect failed (...): -125` ya da `read failed: -125`
  (MEMORY_E); `-303` MEMORY_ERROR, `-2` MP_MEM.

- [x] CrossPoint Sync (varsayılan sunucu sync.crosspointreader.com): konum gönderip alın,
  çalışmalı. Seri günlükteki `[SecureClient] handshake ok (...) in N ms` sürelerini ve tüm
  eşitlemenin süresini önceki sürümle karşılaştırın (yukarıdaki ek süre beklenir).
- [x] OPDS: herkese açık bir katalogda (ör. `https://m.gutenberg.org/ebooks.opds/`) gezinip
  bir kitap indirin.
- [ ] Plugin Hub kataloğu ve kurulumu, Manage Fonts'tan yazı tipi indirme (GitHub) ve LCP
  korumalı bir kitabın indirilip açılması çalışmalı.
- [ ] Varsa OAuth ile oturum açılan bir OPDS kataloğu: ayar açıkken giriş çalışmalı. Giriş
  sunucusunun sertifikası reddedilirse "Authentication failed" değil "Server certificate not
  trusted" görünmeli.
- [ ] Başka sunuculara HTTPS ile bağlanan bir eklenti (ör. ödünç kitap eklentisi): ayar açıkken
  geçerli sertifikalı sunucularda çalışmalı.
- [ ] Kendinden imzalı ya da IP adresiyle erişilen bir HTTPS ev sunucusu (Calibre-Web, COPS,
  KOSync `https://<ip>:7200`): ayar açıkken OPDS ve CrossPoint Sync "Server certificate not
  trusted" göstermeli (Sync Clippings açıkken de aynı mesaj); ayar kapalıyken eskisi gibi
  çalışmalı. `http://` ile erişilen sunucular etkilenmez. Sonra ayarı yeniden açın.
- [ ] badssl.com erişilebiliyorsa OPDS sunucusu olarak `https://expired.badssl.com/`,
  `https://wrong.host.badssl.com/`, `https://self-signed.badssl.com/` ve
  `https://untrusted-root.badssl.com/` ekleyin: hepsi "Server certificate not trusted"
  vermeli. `https://ecc384.badssl.com/` sertifika hatası vermemeli (sayfa OPDS olmadığı için
  akış hatası verir).
- [ ] Saat: Settings → System → Clock doğru olmalı; sertifika tarihleri bu saatle denetlenir.
  Saat hiç ayarlanmamışsa Wi-Fi bağlıyken ilk doğrulanan bağlantı SNTP için en çok 8 s bekler;
  SNTP yanıt vermezse günlükte `Clock not set` görünür ve bu açılışta bir daha beklenmez.
- [ ] TLS 1.3 isteyen bir KOSync sunucusu (ör. `https://kosync.ak-team.com:3042`) bağlanmalı.
- [ ] Bellek: eşitleme ve OPDS indirmesi sırasında günlükte yukarıdaki bellek hataları ya da
  `not checked: low memory` olmamalı, cihaz yeniden başlamamalı.
- [ ] Şifre takımı: `handshake ok` satırındaki takım artık `TLS_AES_256_GCM_SHA384` olabilir
  (SHA-384 açıldı); büyük bir kitabın indirme süresi önceki sürümden belirgin uzun olmamalı.

## Saat eşitlemesi ("fix: trustedtime::syncNow reports the sync it waited for")

`syncNow()` başarılı bir SNTP eşitlemesini başarısız sayıyordu (ESP-IDF eşitleme durumunu
okurken sıfırlıyor, işlev onu iki kez okuyordu). Bunu yalnızca süreli (ödünç) LCP kitabı açarken
saat doğrulanamadığında çalışan akış kullanır.

- [ ] Saati doğrulanmamış bir cihazda (ör. saat hiç ayarlanmamış ya da NVS silinmiş) süreli bir LCP
  kitabını açın, Wi-Fi'yi seçin: "Syncing time..." ardından "Sync failed" (Türkçe:
  "Eşitleme başarısız") penceresi çıkmamalı, kitap yeniden açılmalı.

## İki yazılım yan yana ("feat: start the firmware in the other OTA slot")

SD kart güncellemesi yeni yazılımı boştaki yuvaya yazar; eskisi (ör. CrossPoint) diğer yuvada
kalır. Settings → System → Start Other Firmware (Diğer yazılımla başlat), o yazılımı denetleyip
(segmentler, sağlama, SHA-256, çip, kart) yeniden yüklemeden onunla yeniden başlatır. Kurtarma
kipi (Aşağı + Güç ile açılış) önce aynı seçeneği sunar, sonra SD karttan dosya seçtirir.
Emülatörde: geçiş yazılan flash soğuk açılışta CrossPoint'i açtı; bozuk bir yuva "The other
firmware is damaged" ile reddedildi; SD güncellemesi CrossPoint imajını app1'e birebir yazdı.
Emülatör yazılımla yeniden başlatmayı desteklemediği için geçişten sonraki yeniden başlatmanın
kendisi yalnızca cihazda denenebilir.

- [x] CrossPoint çalışırken bu yazılımı Dosya Aktarımı (hotspot) ile SD kartın köküne koyup
  CrossPoint'in SD Card Firmware Update'iyle yükleyin: Bembeyaz açılmalı; ayarlar ve kitaplar
  yerinde olmalı.
- [x] Settings → System'de "Start Other Firmware" görünmeli; onay penceresi "CrossPoint
  <sürüm>" yazmalı. Onaylayınca "Validating firmware..." ve ardından yeniden başlatma: cihaz
  CrossPoint ile açılmalı.
- [x] CrossPoint'te SD Card Firmware Update ile aynı `firmware.bin`'i yeniden seçin: Bembeyaz
  açılmalı. Gidiş-dönüşten sonra kitaplar açılmalı (önbellek yeniden kurulabilir).
- [x] Kurtarma kipi: cihaz kapalıyken Aşağı tuşunu basılı tutup Güç'e basın. Önce "Restart into
  the other firmware?" sorulmalı; Cancel deyince SD dosya seçme ekranı açılmalı.
- [ ] Yukarı tuşu açılışta basılı tutulursa ekran donar (USB indirme modu). Bundan nasıl
  çıkıldığını not edin (güç tuşuna uzun basma, şarj kablosunu çıkarıp takma...).

## Ad: purestWhite / purestWhite next, sürüm 0.0.1

2026-10-20: sahibi X4 Pro'da 2b45110'yu (purestWhite next 0.0.1) CrossPoint'in SD güncellemesiyle
yükleyip işaretli maddeleri denedi; sorun yok.

Yazılımın adı X4 Classic'te "purestWhite", X4 Pro'da "purestWhite next" oldu; sürümü 0.0.1
(sürümde artık "-x4pro" eki yok). Sunucuların ve eklentilerin tanıdığı adlar CrossPoint'te
kaldı: CrossPoint Sync, eklenti ailesi "crosspoint", "CrossPoint-ESP32-" kullanıcı aracısı,
Calibre'nin keşif yanıtı.

- [x] Açılış ve uyku ekranında "purestWhite next" (X4 Pro) yazmalı, açılışta altta "0.0.1".
- [x] Settings → System → About'ta Firmware satırı "purestWhite next 0.0.1" olmalı.
- [x] Dosya Aktarımı → Create Hotspot: telefonda ağ "purestWhite next" adıyla görünmeli;
  `http://purestwhite.local` açılmalı, sayfa başlığı ve üst bilgi "purestWhite" olmalı.
- [ ] Join a Network ile bağlanınca yönlendiricinin listesinde cihaz "purestWhite-<MAC>"
  adıyla görünmeli; `http://purestwhite.local` açılmalı.
- [ ] Calibre Wireless: Calibre'deki CrossPoint Reader eklentisi cihazı yine bulmalı.
- [x] CrossPoint Sync ile ilerleme eşitlemesi eskisi gibi çalışmalı.
- [ ] Bilgisayara USB ile bağlanınca aygıt adı "purestWhite_next", üretici "purestWhite"
  olmalı (Linux: `lsusb -v`).
- [x] Start Other Firmware: diğer yuvada CrossPoint varsa onay "CrossPoint 1.6.x", bu
  yazılımın bu sürümü ya da sonrası varsa "purestWhite next 0.0.1" yazmalı. Bu sürümden
  önceki Bembeyaz yapıları kimlik taşımadığı için "CrossPoint 1.6.5-x4pro" görünür.

## Logo: papatya

Açılış ve uyku logosu ile web simgesi papatya oldu (`src/images/logo.svg`; cihaz görüntüsü
`scripts/generate_logo.py` ile üretilir, çizgiler 2 piksel).

- [x] Açılış ekranında papatya dik ve ortada görünmeli; çizgiler kopuk olmamalı.
- [ ] Uyku ekranında (koyu) papatya beyaz çizgilerle görünmeli; açık uyku ekranı ayarında da.
- [x] Web sayfasının üst bilgisinde ve tarayıcı sekmesinde papatya simgesi görünmeli.

## TLS oturum sürdürme ("net: SecureClient resumes TLS sessions")

Sunucu oturum bileti gönderirse SecureClient oturumu saklar: en çok 4 oturum (sunucu, port ve
doğrulama ayarı başına; en eski kullanılan atılır), her biri ≈400 B iç RAM ve 512 bayta kadar
bilet; yeniden başlatma, derin uyku ya da biletin süresi dolana kadar. Aynı sunucuya sonraki bağlantı sertifika zincirini yeniden denetlemez; günlükte
`handshake ok (auto): TLSv1.3 / <takım>, resumed in N ms` görünür. Doğrulamalı ve doğrulamasız
oturumlar ayrı tutulur; sabitlenmiş kök sertifikalı bağlantılar ve TLS 1.2 yeniden denemesi
sürdürülmez. Emülatörde (gerçek geçen süre; cihazda farklı olabilir) ikinci bağlantı: sync.koreader.rocks
1102 → 194 ms, kosync.ak-team.com 2452 → 788 ms, www.google.com 973 → 177 ms, yalnızca TLS 1.2
konuşan yerel bir sunucu 3091 → 56 ms. sync.crosspointreader.com ve github.com sürdürmedi. Dört
yuva doluyken oturumlar ≈2,5 KB iç RAM tutar; en büyük boş blok değişmedi.

- [ ] Yeniden başlattıktan sonra KOSync sunucusunu sync.koreader.rocks ya da
  `https://kosync.ak-team.com:3042` yapıp iki kez eşitleyin: ilk `handshake ok` satırında "resumed" yok, sonrakilerde `, resumed` ve çok daha
  kısa süre olmalı; eşitleme çalışmalı.
- [ ] Varsayılan sunucu sync.crosspointreader.com: "resumed" beklenmez; eşitleme eskisi gibi
  çalışmalı.
- [ ] Yeniden başlatın, Verify Server Certificates'ı kapatın ve sync.koreader.rocks ile
  eşitleyin: ilk bağlantıda "resumed" olmamalı, sonrakilerde olabilir. Ayarı açıp yeniden
  eşitleyin: bu açılışta ayar açıkken o sunucuya bağlanılmadıysa ilk bağlantıda "resumed"
  olmamalı. Sonunda ayarı açık bırakın.
- [ ] Derin uyku ya da yeniden başlatmadan sonra aynı sunucuya ilk bağlantıda "resumed"
  görünmemeli (tam el sıkışma).
- [ ] Bellek: 4'ten çok farklı HTTPS sunucusuna bağlanın (eşitleme, OPDS, Plugin Hub/GitHub,
  bir eklenti); günlükte `-125`/`-303` ya da `not checked: low memory` olmamalı, cihaz yeniden
  başlamamalı.
- [ ] Varsa yalnızca TLS 1.2 konuşan bir ev sunucusu: ikinci bağlantıda `TLSv1.2 ..., resumed`
  görünebilir; her durumda bağlanmalı.

## KOSync: eşitleme başına tek bağlantı ("net: one KOSync connection per sync")

Bir eşitlemenin istekleri (açıksa alıntı eşitlemesi, konum alma, konum gönderme) sunucuya tek bir
bağlantıdan gider. Akıllı eşitlemede (Smart Sync, varsayılan) konum alındıktan hemen sonra
gönderilir; Ask Every Time ile sonuç ekranında seçilir. İkisinde de oturum sürdürmeyen varsayılan
sunucuda (sync.crosspointreader.com) gönderme ayrı bir tam el sıkışma (≈1,6–2,2 s) yapmaz. Sunucu
boş bağlantıyı ≈57 s açık tutar (sync.koreader.rocks ≈17 s); daha uzun beklenirse bağlantı
yeniden açılır (emülatörde, bağlantıyı 15 s'de kapatan bir sunucuyla hatasız). Bağlantı sonuç
ekranı ya da "No remote progress found" ekranı açıkken ≈7,5 KB iç RAM tutar; konum gönderilince,
sunucudaki konum uygulanınca, konumlar zaten eşitse, eşitleme başarısız olunca ya da eşitleme
ekranından çıkılınca (cihaz yeniden başlar) kapanır.

- [ ] Smart Sync açıkken, cihazdaki konum sunucudakinden ilerideyken varsayılan sunucuyla
  eşitleyin: günlükte tek bir `handshake ok` satırı olmalı ve konum gönderilmeli.
- [ ] Settings → CrossPoint Sync → Sync Behavior: Ask Every Time yapın; eşitleyip sonuç ekranında
  birkaç saniye içinde "Upload local progress" seçin: konum alma ile gönderme arasında ikinci bir
  `handshake ok` satırı olmamalı, gönderme hızlı bitmeli.
- [ ] Aynısını sonuç ekranında bir dakikadan uzun bekleyip yapın: ikinci bir `handshake ok`
  görünür; gönderme yine başarılı olmalı.
- [ ] Sunucuda konumu olmayan bir kitapla (Ask Every Time) "No remote progress found" ekranında
  "Upload" seçin: ikinci bir `handshake ok` olmamalı; gönderme başarılı olmalı, bellek hatası
  olmamalı.
- [ ] Sync Clippings açıkken varsayılan sunucuyla eşitleyin: alıntılardan konum almaya kadar
  günlükte tek bir `handshake ok` satırı olmalı; alıntılar, sonuç ekranı ve gönderme çalışmalı.
- [ ] Sonuç ekranında sunucudaki konum bölüm/sayfa olarak doğru gösterilmeli (kitap bağlantı
  açıkken yeniden yüklenir); günlükte bellek hatası (`-125`/`-303`) olmamalı, cihaz yeniden
  başlamamalı. Sonunda Sync Behavior'ı Smart Sync'e geri alın.

## Gövdesiz yanıtlar ("fix: 204, 304 and HEAD replies end at their headers")

204 ve 304 yanıtları ile HEAD isteklerinin yanıtı gövde taşımaz. İstemci, uzunluk başlığı yoksa
bunların gövdesini bağlantı kapanana kadar bekliyordu; bağlantıyı açık tutan bir sunucuda bu
15 s'lik zaman aşımı demekti. Spring tabanlı KOSync sunucuları (BookLore, grimmory) konumu
olmayan bir kitap için 204 döner. Host testi, açık tutulan bir bağlantıda üçünün de beklemeden
bittiğini ve bağlantının yeniden kullanıldığını sınar.

- [ ] Varsa Spring tabanlı bir KOSync sunucusunda (BookLore, grimmory) sunucuda konumu olmayan bir
  kitabı eşitleyin: "No remote progress found" beklemeden (≈15 s değil, hemen) görünmeli.
- [ ] Varsayılan sunucuyla eşitleme, OPDS ve Plugin Hub eskisi gibi çalışmalı.

## Wi-Fi ile güncelleme: kendi sürüm kanalımız ("feat: update over Wi-Fi from bdrtr/purestWhite releases")

Settings → System → Check for updates artık görünür ve github.com/bdrtr/purestWhite'ın en son
yayımlanmış sürümünden `purestWhite-<sürüm>-<kart>.bin` dosyasını indirir. Sürümler
`scripts/release.sh` ile taslak olarak hazırlanır, donanım testlerinden sonra elle yayımlanır.
Güncelleme de SD güncellemesi gibi diğer yuvaya yazar: oradaki yazılımın (ör. CrossPoint)
yerini alır. Emülatörde sahte bir sürüm sunucusuyla: doğru kartın dosyası seçildi, CDN
yönlendirmesi izlendi, 5,4 MB'lık görüntü diğer yuvaya bayt bayt yazıldı ve önyükleme o yuvaya
çevrildi. "v0.0.2" gibi bir etiket de anlaşılır; anlaşılmayan bir etiket hiç önerilmez.

- [ ] Henüz yayımlanmış sürüm yokken Check for updates: hata vermeden "güncel" ya da "güncelleme
  yok" demeli.
- [ ] İlk sürüm yayımlandıktan sonra, daha eski bir sürüm çalışırken Check for updates: yeni sürümü
  göstermeli, indirip kurmalı, cihaz yeni sürümle açılmalı (About'ta sürüm). Önceki yazılım
  diğer yuvada kalmalı (Start Other Firmware onu adlandırmalı).
- [ ] Aynı sürüm çalışırken Check for updates güncelleme önermemeli.
- [ ] Güncellemeyi indirme sırasında Wi-Fi'yi kesin: kurulum yarıda kalmalı, cihaz eski sürümle
  açılmaya devam etmeli.

## JSON ayrıştırıcı Rust'ta ("rust: our own JSON reader in place of the SDK's")

SDK'nın akışlı JSON ayrıştırıcısı (StreamingJsonParser) artık `bembeyaz-json` ile çalışıyor;
`lib/JsonSax` SDK'nın API'sini taşıyor. Kullananlar: OPDS 2 katalogları ve yayın sayfaları, OPDS
kimlik doğrulama belgeleri, eklenti katalogları, LCP lisansları, LCP cihaz kaydı ve kilit açma
yanıtları, Wi-Fi güncelleme denetimi. Host'ta bir fark testi SDK'nın C++ uygulamasını ve Rust'ı
20 000 üretilmiş ve bozulmuş belgede, rastgele parçalarla besleyip her olayı ve hata durumunu
karşılaştırıyor. Emülatörde güncelleme denetiminin JSON'u (sahte bir sürüm sunucusundan) doğru
okundu ve sürüm kuruldu.

- [ ] OPDS 2 (JSON) bir katalogda gezinin, bir yayının ayrıntı sayfasını açın, bir kitap indirin.
  Birkaç paragraflık açıklama görünmeli (2 KB'tan uzun açıklamalar için aşağıdaki "Uzun OPDS
  açıklamaları kesilerek gösteriliyor" bölümüne bakın).
- [ ] Oturum açılan bir OPDS kataloğu varsa giriş çalışmalı (kimlik doğrulama belgesi JSON).
- [ ] Plugin Hub kataloğu listelenmeli; bir eklenti kurulmalı.
- [ ] LCP korumalı bir kitabı ödünç alın/indirin, parolayı girin ve açın (lisans, cihaz kaydı ve
  /unlock yanıtı JSON).
- [ ] Settings → System → Check for Updates hatasız sonuç vermeli.

## Bozuk önbellek dosyası çökertmiyor ("fix: a damaged cache string no longer aborts the reader")

Kitap önbelleğinde bir dizginin uzunluk alanı bozulursa (SD kartta bit bozulması) okuyucu o
uzunlukta bellek isteyip `abort()` ile çöküyordu; kitap her açılışta çökme raporu veriyordu.
Emülatörde `book.bin`'deki başlık uzunluğu 0xFFFFFFF0 yapılınca kitap açılırken cihaz çöktü.
Artık `HalFile`'dan okuyan `readString` dosyanın sonunu aşan uzunluğu reddediyor, `readPod` kesik
dosyada sıfır veriyor; `BookMetadataCache::load` bunu görünce önbelleği yeniden kuruyor. Aynı
bozuk kartla kitap açıldı ("Cache file damaged" ve yeniden kurulum günlükte).

- [ ] Normal kullanımda kitap açma, sayfa çevirme ve kapanıp yeniden açma eskisi gibi olmalı
  (önbellek gereksiz yere yeniden kurulmamalı: günlükte "Cache file damaged" görünmemeli).

## İçindekiler okuyucuları Rust'ta ("rust: our own NCX and nav TOC readers")

Kitabın içindekiler belgesi (EPUB 2 NCX ya da EPUB 3 nav) artık `bembeyaz-toc` ile okunuyor; HTML
varlık tablosu (`&nbsp;`, `&quot;`, `&mdash;` …) `bembeyaz-html`'de ve bölüm metni, Ayrıntılar
açıklaması, sözlük tanımları ve KOReader eşitlemesi de onu kullanıyor. Eski C++ okuyucularına
karşı yazılmış 19 host testi değişmeden geçiyor. Geliştirme sırasında bir fark testi eski ve yeni
okuyucuları 60 000 üretilmiş belgede (yarısı rastgele bozulmuş), rastgele parçalarla karşılaştırdı
(her içindekiler girdisinin adı, derinliği, yolu ve çapası ile her yazma sonucu aynıydı; kasıtlı
bir değişiklik yakalandı); eski tablodaki 253 varlığın hepsi aynı değeri veriyor. Emülatörde nav'lı
bir kitap ve onun yalnızca NCX'li kopyası değiştirilmemiş CrossPoint'le aynı içindekileri (5 girdi)
ve aynı ekranları verdi (`emu/make_xml_epub.py`'nin kitabı).

- [ ] Bir EPUB 3 kitabın önbelleğini silin (kitap açıkken okuma menüsünde Delete Book Cache /
  Kitap Önbelleğini Sil) ya da kitabı ilk kez açın; okuma menüsünde Select Chapter / Bölüm Seç:
  bölümler doğru adlarla ve doğru girintiyle görünmeli, birini seçince o bölüme gitmeli.
- [ ] Aynısını yalnızca NCX'i olan eski bir EPUB 2 kitapla yapın.
- [ ] Bölüm adlarında `&mdash;`, `&hellip;` gibi varlıklar olan bir EPUB 3 (nav'lı) kitapta adlar
  içindekilerde doğru görünmeli.
- [ ] Açıklamasında `&quot;`, `&amp;`, `&mdash;` olan bir kitabın Ayrıntılar ekranında açıklama
  doğru görünmeli (varlık adı değil işaret).
- [ ] KOReader eşitlemesi: konum eskisi gibi aktarılmalı.

## İçindekiler etiketi en çok 1 KB ("fix: a TOC label keeps at most 1 KB")

İçindekiler girdisinin adı artık en çok 1024 bayt tutuluyor (tam bir karakterde biter); eskiden
sınırsızdı ve megabaytlarca metinli bozuk bir içindekiler belgesi kopyalanırken cihazı
çökertebiliyordu. Gerçek bölüm adları bunun çok altındadır.

- [ ] Uzun bölüm adlı bir kitapta içindekiler eskisi gibi görünmeli.

## Uzun OPDS açıklamaları kesilerek gösteriliyor ("fix: a long JSON string value is cut, not dropped")

JSON ayrıştırıcısının 2047 baytlık belirteç tamponuna sığmayan bir dizgi değeri SDK'daki gibi
tamamen atılıyordu: 2 KB'tan uzun bir OPDS 2 kitap açıklaması hiç görünmüyordu. Artık değer bir
UTF-8 karakter sınırında kesilerek veriliyor; çoğu okuyucu alanlarını zaten kendi sınırına
kısaltıyor. Anahtarlar ve sayılar eskisi gibi atılıyor. Değeri bütün saklayan okuyucular (OAuth
jetonu, eklenti kataloğu) böyle uzun bir jetonu ya da adresi kesik alır: giriş ya da indirme yine
başarısız olur, ama hemen değil, istekte.

- [ ] Açıklaması 2 KB'tan uzun (yayın JSON'unda description 2048 bayttan uzun) bir OPDS 2
  yayınının ayrıntı ekranında "About this book" altında açıklamanın başı görünmeli (boş değil);
  Details ile açılan açıklamanın son sayfasında yarım ya da bozuk karakter olmamalı.
- [ ] HTML biçimli uzun bir açıklamanın sonunda yarım etiket ya da varlık metni görünmemeli.
- [ ] Oturum açılan (OAuth) bir OPDS kataloğunda giriş eskisi gibi çalışmalı.
- [ ] OPDS 2 katalogda gezinme, indirme ve Plugin Hub (katalog ve eklenti indirme) eskisi gibi
  çalışmalı.

## Sürüm 0.0.2 ("release: 0.0.2")

0.0.1'den bu yana: TLS oturum sürdürme, KOSync'te eşitleme başına tek bağlantı, 204/304/HEAD
yanıtlarında bekleme yok, Wi-Fi ile güncelleme kanalı (bdrtr/purestWhite sürümleri), JSON ve
içindekiler okuyucuları Rust'ta, bozuk önbellek dizgisi çökertmiyor, içindekiler etiketi en çok
1 KB, uzun OPDS açıklamaları kesilerek gösteriliyor. Bu sürümün dosyaları GitHub'da taslak olarak
duruyor; yukarıdaki işaretsiz maddeler denendikten sonra yayımlanacak.

- [ ] About'ta Firmware satırı "purestWhite next 0.0.2" olmalı, açılışta altta "0.0.2".
- [ ] Taslak yayımlandıktan sonra 0.0.1 çalışan bir cihazda Check for Updates 0.0.2'yi önermeli.

## container.xml Rust'ta ("rust: our own container.xml reader")

Kitabın paket belgesinin yerini söyleyen `META-INF/container.xml` artık `bembeyaz-opf`'in
`container` modülüyle okunuyor. Eski C++ okuyucusuna karşı yazılmış 10 host testi değişmeden
geçiyor. Geliştirme sırasında bir fark testi eski ve yeni okuyucuyu 50 000 üretilmiş belgede (bir
kısmı bozulmuş; 5 627'sinde bir yol bulundu) rastgele parçalarla karşılaştırdı (bulunan yol ve her
yazma sonucu aynıydı; kasıtlı bir değişiklik yakalandı). Emülatörde `emu/compare.sh` değiştirilmemiş
CrossPoint'le aynı ekranları verdi.

- [ ] Farklı kaynaklardan birkaç kitabın önbelleğini silin (kitap açıkken okuma menüsünde Delete
  Book Cache / Kitap Önbelleğini Sil) ya da kitapları ilk kez açın: kitaplar açılmalı, kütüphanede
  başlık ve kapak görünmeli; seri günlükte "Could not find valid rootfile in container.xml"
  görünmemeli.
- [ ] KOReader eşitlemesi eskisi gibi çalışmalı (o da container.xml okur).

## Tablo sütun genişlikleri Rust'ta ("rust: grid table column widths")

Bir tablonun satırları dizilmeden önce yapılan ön tarama (her sütunun en uzun sözcüğü ve en uzun
kırılmamış satırı) ve ekran genişliğinin sütunlara paylaştırılması artık `bembeyaz-layout`'un
`table` modülünde. Bölüm ayrıştırıcısının tablo testleri dahil 890 host testi geçiyor. Geliştirme
sırasında bir fark testi Rust kodunu eski C++ koduyla 300 000 üretilmiş (kimi bozuk, kimi 32 KB
sınırını aşan) tabloda ve dört kitaptan 101 gerçek tabloda karşılaştırdı: ölçüm ve genişlikler
aynıydı. Emülatörde `emu/compare.sh`, `test/epubs/crosspoint-table-edge-cases.epub` ile 12 sayfa
boyunca değiştirilmemiş CrossPoint'le aynı ekranları verdi.

- [ ] `test/epubs/crosspoint-table-edge-cases.epub`'ı karta kopyalayıp açın (daha önce açtıysanız
  okuma menüsünden Delete Book Cache / Kitap Önbelleğini Sil): sütunlar içeriğe göre geniş ya da
  dar olmalı; "Four columns" tablosu dört sütunu yan yana göstermeli; "Too many columns"
  tablosundaki satırlar hiçbir hücre kaybolmadan alt alta dizilmeli.
- [ ] Tablolu bir kitabı (ör. bir programlama kitabı) önbelleğini silip yatay ekranda ve daha
  büyük yazıyla açın: tablo sütunları ekrana sığmalı, cihaz takılmamalı.

## BMP okuyucusu Rust'ta ("rust: our own BMP reader")

Uyku ekranı resimleri, resim görüntüleyici ve kapak küçük resimleri için BMP dosyalarının başlığı
ve paleti artık `bembeyaz-bmp` ile okunuyor, satırları onunla parlaklığa çevriliyor; titreme ve
ekranın gri tonlarına çevirme C++'ta kaldı. Yeni `test/bmp_reader` testleri dahil 898 host testi
geçiyor; bu yeni testler eski kodla da geçiyor. Geliştirme sırasında bir fark testi eski C++ kodunu
Rust koduyla 30 000 üretilmiş (kimi bozuk ya da kesik) BMP'de ve kart yedeğindeki 12 gerçek BMP'de,
titremeli ve titremesiz, satır satır karşılaştırdı: sonuçlar aynıydı. Emülatörde
`emu/compare.sh emu/bmp.txt` yedi BMP'de değiştirilmemiş CrossPoint'le aynı ekranları verdi.

- [ ] Kartın kökündeki `sleep` klasörüne birkaç `.bmp` koyun, Ayarlar > Ekran > Uyku Ekranı'nı
  "Özel" (Settings > Display > Sleep Screen > Custom) yapın ve cihazı uyutun: resimler eskisi gibi
  görünmeli.
- [ ] `python3 emu/make_bmp_files.py bmp` ile üretilen yedi dosyayı karta kopyalayıp dosya
  listesinden tek tek açın: resim görüntüleyicide düzgün görünmeli; `7-cut.bmp` yalnızca kısmen
  görünmeli, cihaz takılmamalı.
- [ ] Home'daki kitap kapağı küçük resimleri eskisi gibi görünmeli (önbellek silmek gerekmez: Home
  her çizişte bu BMP'leri yeniden okur).

## Saydam uyku kaplaması BMP başlığını `Bitmap` ile okuyor ("sleep: read the overlay BMP header with Bitmap")

SleepActivity'nin saydam uyku kaplaması için kendi yazdığı BMP başlık okuyucusu kaldırıldı; başlık
artık `Bitmap` (yani `bembeyaz-bmp`) ile okunuyor. Tek fark: başlığında 256'dan fazla palet rengi
bildiren 32 bitlik bir dosya artık kaplama olarak çizilmiyor (normal uyku resmi olarak da zaten
reddediliyordu); bu `test/bmp_reader`'da sabitlendi. Emülatörde `emu/sleep.txt` kaplamayı
değiştirilmemiş CrossPoint'le aynı çizdi.

- [ ] Kartın köküne 32 bitlik, saydam alanları olan bir `sleep-overlay.bmp` koyun (örneğin
  `python3 emu/make_bmp_files.py bmp` ile üretilen `bmp/overlay/card/sleep-overlay.bmp`), Ayarlar >
  Ekran > Uyku Ekranı'nı "Transparent" yapın (Settings > Display > Sleep Screen > Transparent; bu
  seçeneğin Türkçesi yok) ve cihazı Güç düğmesine basılı tutarak uyutun. Üretilen dosyayla
  uyumadan önceki ekranın üstünde, içe doğru solan renkli bir halka görünmeli; halkanın dışında
  önceki ekran olduğu gibi kalmalı.

## XTC/XTCH ayrıştırıcısı Rust'ta ("rust: our own XTC reader")

Xteink'in önceden çizilmiş sayfa biçimi XTC/XTCH'nin dosya başlığı, sayfa tablosu, sayfa başlıkları
ve bölüm listesi artık `bembeyaz-xtc` ile okunuyor; dosyayı açıp kapatmak C++'ta kaldı. Yeni
`test/xtc_parser` testleri dahil 910 host testi geçiyor; bu yeni testler eski kodla da geçiyor.
Geliştirme sırasında bir fark testi eski C++ kodunu Rust koduyla 30 000 üretilmiş (kimi bozuk ya da
kesik) kitapta ve 205 gerçek XTCH kitabında (her sayfası, parça parça okuma ve bölümleriyle)
karşılaştırdı: sonuçlar aynıydı. Emülatörde `emu/compare.sh emu/xtc.txt`, `emu/make_xtc_files.py`
ile üretilen 1 ve 2 bitlik iki kitapta değiştirilmemiş CrossPoint'le aynı ekranları verdi.

- [ ] Birkaç `.xtc` ve `.xtch` kitabı açın: sayfalar eskisi gibi çizilmeli, sayfa çevirmek eskisi
  kadar hızlı olmalı; kitabı açtıktan sonra başlık ve yazar Home'daki son okunanlarda görünmeli.
  `python3 emu/make_xtc_files.py xtc` ile üretilen iki kitapta her sayfada sayfa numarası kadar
  siyah çubuk vardır.
- [ ] Bölümleri olan bir XTCH kitabında bölüm listesini açın: bölümler eskisi gibi listelenmeli,
  bir bölüme atlayınca doğru sayfa açılmalı.
- [ ] Daha önce açılmamış (ya da önbelleği silinmiş) bir XTC kitabının Home'daki kapak küçük resmi
  (kitabın ilk sayfasından üretilir) eskisi gibi görünmeli.

## XTC bölüm listesi için ön ayırma sınırlı ("xtc: bound the chapter list reservation")

Bölüm tablosu dosyanın sonuna kadar uzanan (sayfa tablosu ve sayfa verisi bölümlerden önce gelen)
bir XTC kitabında bölüm listesi, tablonun alabileceği her kayıt için önceden yer ayırıyordu; büyük
bir dosyada bu megabaytlarca bellek demekti ve ayırma başarısız olunca cihaz çökerdi. Artık en çok 256 bölümlük
yer önceden ayrılıyor, liste gerektikçe büyüyor. `test/xtc_parser`'daki yeni test bunu doğruluyor.

- [ ] 256'dan fazla bölümü olan bir XTC ya da XTCH kitabında bölüm listesi eskisi gibi açılmalı,
  son bölümler de görünmeli.

## KOReader bölüm konumları Rust'ta ("rust: our own KOReader XPath resolver")

KOReader eşitlemesinin bir okuma konumunu KOReader'ın anladığı yola
(`/body/DocFragment[3]/body/div[2]/p[4]/text()[1].12` gibi) çeviren kısmı artık `bembeyaz-kosync`'te;
bölümü kitaptan okumak C++'ta kaldı. Bölüm XPath testleri dahil 911 host testi geçiyor. Geliştirme
sırasında bir fark testi eski C++ kodunu Rust koduyla 20 000 üretilmiş bölümde (iç içe öğeler, gizli
öğeler, HTML varlıkları, yorumlar, CDATA, bozuk ya da kesik belgeler) ve dört kitabın 1518 gerçek
bölümünde, paragraf, konum ve ilerleme hedefleriyle karşılaştırdı: sonuçlar aynıydı. Bu kod yalnızca
konum gönderirken çalışır; KOReader'dan konum almak başka koddan geçer.

- [ ] Ayarlar → CrossPoint Sync'te Server Type "KoSync" ve Senkronizasyon Davranışı "Her seferinde
  sor" olsun, hesap bilgileri girilmiş olsun. Bir
  EPUB'da bir paragrafın ortasına ve bir liste maddesine gidin; okuma menüsünden "Okuma
  İlerlemesini Senkronize Et" → "Yerel ilerlemeyi yükle"yi seçin. KOReader'da aynı kitabı açıp
  ilerlemeyi çekin: aynı paragraf ve yaklaşık aynı satır açılmalı. Bir bölümün son sayfasında da
  deneyin.
- [ ] (Değişmeyen yön, eskisi gibi çalışmalı) KOReader'da ilerleyip burada "Uzak ilerlemeyi uygula"yı
  seçin: aynı sayfa açılmalı.

## KOReader konumu uzun bir öğeden sonra da bulunuyor ("kosync: find positions held back to the end of a chapter")

Bir bölümde 1 KB'tan uzun bir öğe (uzun bir SVG yolu, gömülü bir resim, uzun bir stil ya da yorum)
varsa XML ayrıştırıcısı sonrasını bölümün sonuna kadar bekletir. Konum ancak o zaman bulunursa eski
kod onu yok sayıyor, eşitleme bölümün başına ya da daha kaba bir konuma düşüyordu. Artık bulunuyor;
`test/koreader_xpath_resolver`'daki yeni test bunu doğruluyor.

- [ ] İçinde büyük gömülü resimler ya da SVG'ler olan bir bölümün son sayfasında "Okuma İlerlemesini
  Senkronize Et" → "Yerel ilerlemeyi yükle"yi seçin: KOReader'da aynı yer açılmalı.

## KOReader'dan gelen konumu bulmak Rust'ta ("rust: our own KOReader position locator")

KOReader eşitlemesinde sunucudan gelen konum yolunu (`/body/DocFragment[3]/body/div[2]/p[4]/text()[1].12`
gibi) okuyup bölümde bulan kısım artık `bembeyaz-kosync`'in `locate` modülünde; bölümü kitaptan
okumak ve bulunan konumu sayfaya çevirmek C++'ta kaldı. 912 host testi geçiyor. Geliştirme sırasında
bir fark testi eski C++ kodunu Rust koduyla 30 000 üretilmiş bölüm ve yolda (bozuk olanlar dahil)
ve dört kitabın 1518 gerçek bölümünden üretilmiş 64 504 yolda, her arama biçimiyle karşılaştırdı:
sonuçlar aynıydı. Bu kod çoğunlukla konum alırken çalışır; gönderirken yalnızca bölüm XPath
çözücüsü yol bulamazsa son çare olarak kullanılır.

- [ ] Ayarlar → CrossPoint Sync'te Server Type "KoSync" ve Senkronizasyon Davranışı "Her seferinde
  sor" olsun (varsayılan "Akıllı senkronizasyon" bu seçimi sormaz), hesap bilgileri girilmiş olsun.
  KOReader'da bir kitabın ortasında,
  bir liste maddesinde ve bir bölümün başında ilerlemeyi gönderin; burada aynı kitapta okuma
  menüsünden "Okuma İlerlemesini Senkronize Et" → "Uzak ilerlemeyi uygula"yı seçin: her seferinde
  aynı yer açılmalı.

## Kütüphane metni Rust'ta ("rust: our own Library text")

Kütüphanenin arama ve sıralamada kullandığı metin işlevleri (aksanları kaldıran katlama, harf grubu,
yazar adını düzeltme, aynı kişinin farklı yazımlarını birleştiren yazar anahtarı, arama eşleşmesi,
soyadına göre sıralama) artık `bembeyaz-library`'de. Kütüphane metni, kurucusu ve dizin dosyası
testleri dahil 912 host testi geçiyor. Geliştirme sırasında bir fark testi eski C++ kodunu Rust
koduyla 300.000 rastgele metinde (aksanlı, ayrışık, çeşitli yazılar, bozuk baytlar) karşılaştırdı:
sonuçlar aynıydı. Emülatörde Kütüphane'nin "Recent", "Title" ve "Author" sekmeleri değiştirilmemiş
CrossPoint'le aynı göründü. Tek fark: bozuk bir dizin kaydında bir kitabın harf grubu artık kaydın
sınırını aşan baytlardan okunmuyor.

- [ ] Kütüphaneyi (Library) açın: "Title" ve "Author" sekmelerinde kitaplar eskisi gibi sıralanmalı.
  "Title" sekmesinde bir kitabı basılı tutup harf gruplarını açın: gruplar eskisi gibi olmalı; Ç, Ö,
  Ş ile başlayan başlıklar ayrı harf olarak değil, C, O, S grubunda görünür. "Author" sekmesinde
  "Austen, Jane" ve "Jane Austen" yazımlı kitaplar tek yazar altında görünmeli.
- [ ] Aramada (Search) aksansız "cicek" yazın: "Çiçek" başlıklı kitap bulunmalı.

## Kütüphane dizin dosyasını okumak Rust'ta ("rust: our own Library index reader")

Kütüphanenin karttaki dizin dosyasını (`CLX1`) doğrulayan ve okuyan kısım (başlık, bölümler, kitap
kayıtları, adlar, yazar, başlık, klasör yolu, son okunanların sırası) artık `bembeyaz-library`'nin
`format` modülünde; dosyayı açmak ve dizini yazan kurucu C++'ta kaldı. Biçim, dizin dosyası ve
kurucu testleri dahil 912 host testi geçiyor. Geliştirme sırasında bir fark testi eski C++ kodunu
Rust koduyla 20.000 üretilmiş (kimi bozuk ya da kesik) dizin dosyasında karşılaştırdı: sonuçlar
aynıydı.

- [ ] Kütüphaneyi açın, "Recent", "Title" ve "Author" sekmelerinde kaydırın ve bir kitap açın:
  kitaplar, adları, yazarları ve açılan kitap eskisi gibi olmalı.
- [ ] Kütüphane'de "Recent" sekmesini açın (arama boş): son okuduğunuz kitaplar en üstte olmalı ve
  aşağıdaki listede ikinci kez görünmemeli. Sıralamayı ters çevirin: kitaplar tek tek, doğru sırada
  listelenmeli.
- [ ] Home'daki son okunanlar rafındaki başlıklar eskisi gibi görünmeli.
- [ ] Kütüphaneyi açıp kapatın, sonra yeniden açın: ikinci açılışta dizin yeniden kurulmamalı.

## TXT ve Markdown kitaplarını dönüştürmek Rust'ta ("rust: our own text to XHTML")

TXT ve Markdown kitaplarının metnini okunabilir sayfaya çeviren kısım (satır sonları, satır başı
girintileri, boşluklar, `&`, `<`, `>` kaçışları, bayt sırası işareti) artık `bembeyaz-html`'in
`text` modülünde. TXT dönüştürme testleri dahil 912 host testi geçiyor. Geliştirme sırasında bir fark
testi eski C++ kodunu Rust koduyla 100.000 rastgele metinde, rastgele okuma parçalarıyla karşılaştırdı:
sonuçlar aynıydı. Emülatörde girintili, Türkçe karakterli bir TXT kitabı değiştirilmemiş CrossPoint'le
aynı sayfaları verdi.

- [ ] Bir `.txt` ve bir `.md` kitap açın (daha önce açtıysanız kitap açıkken okuma menüsünden Delete
  Book Cache / Kitap Önbelleğini Sil; cihaz Ana ekrana döner, kitabı yeniden açın):
  satır başı girintileri, boş satırlar ve Türkçe karakterler eskisi gibi görünmeli.

## SD kart fontunun başlığını okumak Rust'ta ("rust: our own .cpfont header reader")

Karttaki bir `.cpfont` fontunu açarken başlığını ve stillerin içindekiler tablosunu okuyan, sayı
sınırlarını ve aralık tablosunu denetleyen, bölümlerin dosyadaki konumlarını ve içerik özetini
(bölüm önbelleğindeki font kimliği bundan hesaplanır) çıkaran kısım artık `bembeyaz-font`'un
`cpfont` modülünde; dosya okuma, bellek ayırma, stiller arasında tablo paylaşımı ve glif, kerning,
ligatür okuyucuları C++'ta kaldı. SD font testleri dahil 920 host testi geçiyor. Geliştirme
sırasında bir fark testi eski C++ kodunu Rust koduyla 60.000 üretilmiş (kimi bozuk ya da kesik) font
dosyasında ve dört gerçek fontta (dört stilli Liberation Serif, Noto Sans Latin/Yunan/Kiril, Noto
Serif ASCII, 10 MB'lık Noto Sans CJK) ve bunların 9.200 bozulmuş kopyasında karşılaştırdı: sonuçlar
aynıydı. Emülatörde bir SD fontuyla okunan kitap ve bozuk bir fonttan yerleşik fonta dönüş
değiştirilmemiş CrossPoint'le aynı göründü.

- [x] Ayarlar → Manage Fonts / Yazı Tiplerini Yönet'ten indirilmiş ya da `/.fonts` (veya `/fonts`)
  altına konmuş bir fontu Text Settings / Metin ayarları → Font / Yazı tipi'nde okuma fontu olarak
  seçin ve bir kitap açın: metin o fontla, kalın ve italik yerler doğru stille görünmeli.
- [x] Bu fontla bir kitap açın, kapatın, cihazı yeniden başlatıp kitabı yeniden açın: bölümler
  yeniden oluşturulmadan hemen açılmalı (günlükte `Parameters do not match` görünmemeli). Cihaz
  5f321eb'den eski bir sürümden geliyorsa, yüklemeden sonraki ilk açılışta kitap önbelleği
  `BOOK_CACHE_VERSION` artışı yüzünden bir kez yeniden kurulur.
- [ ] Çince, Japonca ya da Korece bir SD fontu varsa onunla bir kitap açın: karakterler eskisi gibi
  görünmeli.

## SD kart fontunun kerningi Rust'ta ("rust: our own .cpfont kern reader")

Karttaki bir fontla okurken her sayfa için yalnızca o sayfanın harf çiftlerini içeren mini kerning
matrisini kuran kısım (sınıf tablolarını 64 girdilik bloklar halinde okuma, sonraki sayfalarda
gereksiz blokları atlayan blok dizini, sınıfları yeniden numaralama, matris satırlarını okuma) artık
`bembeyaz-font`'un `kern` modülünde; dosyayı açıp okumak (okuma ve konumlama geri çağrıları),
tablolar için bellek ayırmak, ligatür okuyucusu (tabloyu ayrıştırmadan kopyalar) ve glif
okuyucuları C++'ta kaldı. Bozuk kerning ve ligatür verisi için altı yeni test (0bd8c048) dahil 926
host testi geçiyor. Geliştirme sırasında bir fark testi eski C++ kodunu Rust koduyla 120.000
üretilmiş durumda (rastgele, kimi bozuk, kesik ya da sırasız sınıf tabloları; bellek ayırma
hataları) ve dört gerçek fontta 7.470 sayfada karşılaştırdı: sonuçlar, tablolar ve dosya okuma ve
konumlama sırası aynıydı. Emülatörde SD fontuyla okunan sayfalar ve günlükteki kerning satırları
değiştirilmemiş CrossPoint'le aynıydı.

- [ ] Karttaki bir fontu okuma fontu olarak seçin (Liberation Serif `.cpfont`'u rust/README.md'deki
  SD kart fontu komutuyla üretilebilir). Aynı sayfada "To"/"Ta" ile "Th", "AV"/"AT" ile "AN"
  çiftlerini karşılaştırın: T'nin kolu o ve a'nın üstüne girmeli, A ile V birbirine yaklaşmalı; "Th"
  ve "AN" normal aralıklı kalmalı, harfler üst üste binmemeli. Bunu kitabın ilk açılan sayfasında,
  birkaç sayfa sonra ve geri dönünce; kalın ve italik bir yerde; Türkçe harfli ("Tü", "Yı") bir
  sayfada yapın. Yazı boyutunu değiştirip geri dönün ve yeniden bakın.
- [ ] Geliştirme derlemesiyle (`pio run -e x4pro`; yayın derlemeleri bu satırları yazmaz) seri
  günlük açıkken (`pio device monitor`) bir bölümde birkaç sayfa ileri, sonra geri gidin:
  sayfa çevirdikçe `[SDCF] Built mini kern: N×M matrix` satırları görünmeli; `Failed to read kern
  classes`, `Failed to allocate mini kern`, `Failed to seek to kern row` ya da `Failed to read kern
  row` görünmemeli.

## Bitmap'i boyutundan kısa SD font glifleri boş çizilir ("fix: draw nothing for .cpfont glyphs …")

Bir `.cpfont` glifi `genişlik × yükseklik` pikselden az bitmap baytı bildirirse çizici bitmap'in
sonunu aşıp bellekteki başka baytları ekrana çiziyordu (bozuk ya da kötü niyetli bir fontta).
Böyle bir glif artık boş çizilir; ilerleme genişliği korunduğu için sayfa düzeni değişmez.
Dönüştürücünün ürettiği dört gerçek fontun 75.825 glifinin hiçbiri bu durumda değil, yani sağlam
fontlarda hiçbir şey değişmez. Yeni bir test dahil 927 host testi geçiyor. Emülatörde "e" glifi
bozulmuş bir fontla eski sürüm "e" yerine çizgili lekeler çizdi, bu sürüm boşluk bıraktı; sağlam
fontla sayfalar değiştirilmemiş CrossPoint'le aynı.

- [x] Karttaki sağlam bir fontla birkaç sayfa okuyun: harfler eksiksiz görünmeli (hiçbir harf boş
  kalmamalı).

## SD kart fontunun glif okuyucuları Rust'ta ("rust: our own .cpfont glyph readers")

Karttaki bir fontun glif kayıtlarını çözen kısım (bitmap'i boyutundan kısa glifin boş çizilmesi
dahil), bu kayıtların üç yerde okunması (sayfa önbelleği, düzen için ilerleme genişliği tablosu ve
sayfa önbelleğinde olmayan bir glifin anlık yüklenmesi) ve sayfa bitmap'lerinin tampona okunması
artık `bembeyaz-font`'un `glyph` modülünde; dosya okuma, bellek ayırma ve tampon politikası,
okuma sıralarının sıralanması, anlık yüklenen glifin bitmap okuması ve kod noktası araması C++'ta
kaldı. Yarıda kesilmiş glif tablosu ve bitmap'ler için dört yeni test (e3faa164) dahil 931 host
testi geçiyor. Geliştirme sırasında bir fark testi eski C++ kodunu Rust koduyla 20.000 üretilmiş
fontta (kimi boyutları bozuk, bitmap'i kısa ya da dosyası kesik; bellek ayırma hataları da
benzetildi) toplam 160.240 adımda, ayrıca dört gerçek fontta karşılaştırdı: dönüş değerleri, glif
ölçüleri, bitmap baytları, ilerleme genişlikleri ile dosyadaki okuma ve konumlama sırası aynıydı.
Emülatörde sağlam ve bozuk fontlarla okunan sayfalar ve SD font günlük satırları değiştirilmemiş
CrossPoint'le aynıydı; kısa bitmap'li fontla ekranlar bir önceki sürümle piksel piksel aynıydı.

- [x] Karttaki bir fontla daha önce bu fontla açılmamış bir kitap açın ya da yazı boyutunu
  değiştirip geri dönün (bölüm yeniden dizilir); birkaç sayfa ileri ve geri gidin: harfler, kalın
  ve italik yerler eksiksiz görünmeli, sözcükler üst üste binmemeli, satırlar sağ kenarı aşmamalı.
- [ ] CJK karakterli bir SD fontunu (ör. Noto Sans CJK) okuma fontu yapıp Çince ya da Japonca bir
  kitabı ilk kez açın ve birkaç bölüm ilerleyin: geliştirme derlemesinin (`pio run -e x4pro`)
  seri günlüğünde `[SDCF] Overflow: loaded U+…` satırları görünmeli (sayfa önbelleğinde
  olmayan glifler böyle yüklenir); karakterler eksiksiz görünmeli, boş kare ya da � çıkmamalı.
- [ ] Seri günlükte `Prewarm: failed`, `short glyph read`, `short bitmap read`, `Overflow: failed`
  ya da `buildAdvanceTable: failed` satırları görünmemeli.

## SD kart fontunun aralık tablosu Rust'ta ("rust: our own .cpfont interval tables")

Karttaki bir font yüklenirken her stilin aralık tablosu (hangi karakterin hangi glife karşılık
geldiği) artık `bembeyaz-font`'un `intervals` modülünde okunuyor ve denetleniyor. Uygunsa tablo 16
bitlik sıkışık biçime çevriliyor; aralıkları aynı olan stiller tek bir tabloyu paylaşıyor. Bir
karakterin glifini bu tabloda arayan kısım da aynı modülde. Tablonun belleği (PSRAM) eskisi gibi
C++'ta ayrılıyor. Aralıkları aynı olup biri 65.535'ten çok glifli olduğu için sıkışık biçime
sığmayan iki stil ve boş tablolu stiller için iki yeni test (19d1638d), ayrıca ikinci okumada SD
hatası olunca tablo belleğinin serbest bırakıldığını denetleyen bir test dahil 934 host testi
geçiyor. Geliştirme sırasında bir fark testi eski C++ kodunu Rust koduyla karşılaştırdı:
yüklemede 60.000 üretilmiş font, dört gerçek font ve bunların bozulmuş kopyalarında, benzetilmiş
bellek ayırma ve okuma hataları dahil, tablolar, paylaşımlar ve dosyadaki okuma sırası aynıydı;
kerning okuyucusunda 127.470 kurulumda, glif okuyucularında 160.240 adımda sonuçlar aynıydı.
Emülatörde sağlam, bozuk ve kısa bitmap'li fontlarla ekranlar ve SD font günlük satırları önceki
sürümle aynıydı.

- [ ] Liberation Serif `.cpfont`'unu (rust/README.md'deki komutla üretilir) okuma fontu seçin ya
  da onunla bir kitap açın: geliştirme derlemesinin (`pio run -e x4pro`) seri günlüğünde `Style 1:
  sharing style 0's 28-interval table`, `Style 2: …` ve `Style 3: …` satırları görünmeli; kalın,
  italik ve kalın-italik yerlerde harfler doğru ve eksiksiz olmalı, yanlış karakter çıkmamalı.
- [ ] Bir CJK fontuyla Çince ya da Japonca bir kitapta birkaç sayfa okuyun: karakterler eksiksiz
  ve doğru görünmeli.
- [ ] Seri günlükte `invalid interval`, `Failed to read interval`, `Failed to read compact
  interval`, `Failed to allocate … intervals` ya da `Failed to seek … intervals` satırları
  görünmemeli.

## SD kart fontunun sayfa kümesi Rust'ta ("rust: our own .cpfont page codepoint set")

Bir sayfa çizilmeden önce hangi gliflerin bellekte tutulacağını belirleyen veri mantığı artık
`bembeyaz-font`'un `page` modülünde: sayfa metninden benzersiz kod noktalarının toplanması (en çok
512, U+FFFD ve sayfada iki parçası da bulunan ligatürlerin çıktı kod noktaları dahil), bellekteki
sayfanın yeni isteği zaten karşılayıp karşılamadığının denetimi, ikisinin birleştirilmesi, kod
noktalarının gliflere eşlenmesi ve mini aralık tablosunun kurulması. Sıralama, bellek ayırma ve
tampon politikası C++'ta kaldı. Dört yeni pin testi (1124c056) ve kod noktası sırası testi dahil
939 host testi geçiyor. Geliştirme sırasında bir fark testi eski C++ kodunu Rust koduyla 20.000
üretilmiş fontta (ligatürlü, 512'den çok kod noktalı ve çok metinli sayfalarla) toplam 160.152
adımda, ayrıca dört gerçek fontta karşılaştırdı: dönüş değerleri, glif ölçüleri, bitmap baytları,
ilerleme genişlikleri ile dosyadaki okuma ve konumlama sırası aynıydı. Emülatörde sağlam, bozuk ve
kısa bitmap'li font kartlarıyla ekranlar ve SD font günlük satırları önceki sürümle aynıydı.

- [ ] Karttaki bir fontu okuma fontu seçip Latin bir kitapta birkaç sayfa ileri ve geri gidin, bir
  sayfayı yeniden açın: harfler eksiksiz görünmeli; geliştirme derlemesinin (`pio run -e x4pro`)
  seri günlüğünde `[SDCF] Overflow: loaded U+…` satırları önceki sürüme göre artmamalı (sayfanın
  glifleri önceden yüklenir), `Prewarm: failed` ya da `Failed to allocate mapping array` satırları
  görünmemeli.
- [ ] Ligatürlü bir SD fontunu (ör. sd-fonts.yaml'deki Literata; rust/README.md'deki Liberation
  Serif `.cpfont`'unda ligatür yok) okuma fontu seçip "fi", "fl" geçen bir sayfa açın: günlükte
  `[SDCF] Ligatures loaded: N` (N > 0) görünmeli, `Overflow: loaded U+FB01` ya da `U+FB02`
  görünmemeli; bağlar tek glif olarak çizilmeli.
- [ ] Yunanca kapsayan ve 12 pt boyutu olan bir SD ailesini (ör. Literata) okuma fontu seçip bölüm
  başlıkları Yunanca bir kitabın bölüm listesini açın ve aşağı kaydırın (liste, yerleşik fontta
  olmayan harfleri bu fontla çizer): satırlar eksiksiz görünmeli, boş kare çıkmamalı.

## SD kart fontunun ilerleme genişliği tablosu Rust'ta ("rust: our own .cpfont advance table")

Bölüm dizilirken her harfin genişliğini SD karttan tekrar tekrar okumamak için tutulan tablonun
mantığı artık `bembeyaz-font`'un `advance` modülünde: sözcüklerden benzersiz kod noktalarının
toplanması (küçük büyük harfli metinde harflerin büyük biçimleri, boşluk ve tire dahil), tabloda
arama, fontta olmayan harflere U+FFFD'nin genişliğinin verilmesi ve sınırlı birleştirme. 4096 ve
768 sınırları, tablonun belleği, büyütülmesi ve sıralamalar C++'ta kaldı. Küçük büyük harf eşlemesi
(`utf8SmallCapsUpper`) de artık yalnızca Rust'ta; tüm Unicode aralığında eski C++ ile aynı olduğunu
bir test denetliyor. Üç yeni pin testi (77ce50f5) ve bu eşleme testi dahil 943 host testi geçiyor.
Geliştirme sırasında bir fark testi eski C++ kodunu Rust koduyla 20.000 üretilmiş fontta
(paketlenmiş sözcükler, küçük büyük harfler, boşluk, tire, ek metin ve benzersiz kod noktası 4096'yı
aşan metinlerle) toplam 160.248 adımda, ayrıca dört gerçek fontta karşılaştırdı: dönüş değerleri,
glif ölçüleri, bitmap baytları, ilerleme genişlikleri ile dosyadaki okuma ve konumlama sırası
aynıydı. Emülatörde ekranlar ve SD font günlük satırları değiştirilmemiş CrossPoint'le aynıydı;
bölüm dizme süresi değişmedi (önceki sürümde 2.508, bu sürümde 2.512 ms).

- [x] Karttaki bir fontu okuma fontu seçip daha önce bu fontla açılmamış bir kitap açın ya da yazı
  boyutunu değiştirip kitaba dönün (bölüm yeniden dizilir): sözcükler üst üste binmemeli, satırlar
  sağ kenarı aşmamalı, iki yana yaslı satırlarda boşluklar eskisi gibi olmalı.
- [ ] `python3 emu/make_css_epub.py emu/books/css` ile üretilen `css-test.epub`'ı karta kopyalayın;
  Gömülü Stil açıkken karttaki bir fontu okuma fontu seçip açın: küçük büyük harfli "Stillerle
  düzen" başlığı taşmamalı, harfler üst üste binmemeli ("i" eskisi gibi "I" olur, "İ" değil).
- [ ] Geliştirme derlemesinin (`pio run -e x4pro`) seri günlüğündeki `Time to parse and build
  pages` süresi önceki sürümden belirgin uzun olmamalı; `buildAdvanceTable: unique codepoint cap`,
  `failed to allocate mappings` ya da `mergeIntoAdvanceTable: alloc failed` satırları görünmemeli.

## Yerleşik sıkıştırılmış fontlar Rust'ta ("rust: our own compressed font page cache")

Firmware'e gömülü sıkıştırılmış okuma fontlarından (Noto Sans 12–18 pt; arayüzün Noto Sans 8 ve
Ubuntu fontları sıkıştırılmamış, bu yoldan geçmez) bir sayfa için gereken glifleri açan
`FontDecompressor`'ın veri mantığı artık `bembeyaz-font`'un `compressed` modülünde: sayfa
metninden gerekli gliflerin (ligatür çıktıları dahil) toplanması, bunları tutan grupların bulunması,
her glifin açılmış gruptaki yeri, satırları bayta hizalanmış bitmap'ten satır dolgusunun atılması
ve sayfa tablosunun kurulup aranması. Bellek ayırma, sıkıştırmayı açma (ROM'daki tinfl), sayfa
yuvaları ve anlık grup önbelleği C++'ta kaldı. Sayfa önbelleği, `glyphToGroup` tablolu fontlar,
ligatürler, yuva sınırı, bozuk grup, grupta olmayan glif ve sıkıştırılmamış font için yedi yeni test
(44073139) dahil 950 host testi geçiyor. Geliştirme sırasında bir fark testi eski C++ kodunu Rust
koduyla firmware'in 21 yerleşik fontunda (16'sı sıkıştırılmış), sıkıştırılmışların `glyphToGroup`
tablolu (gruplar aynı) ve bozuk kopyalarında, bellek ayırma hataları benzetilerek 129.526 adımda
karşılaştırdı: dönüş değerleri, bitmap'ler ve istatistikler aynıydı. Emülatörde okuma ekranları
değiştirilmemiş CrossPoint'le, ayarlar ekranları bir önceki sürümle aynıydı; sayfa çizme süresi bir
önceki sürümden 2–3 ms uzundu (1.256–1.257 ms yerine 1.258–1.260 ms, ~%0,2; fark çizim
geçişlerinde).

- [ ] Ayarlar'da Okuyucu Yazı Tipi Ailesi Noto Sans iken dört boyutta (12, 14, 16, 18) birkaç
  kitapta sayfa çevirin: harfler, kalın, italik ve kalın italik yerler eksiksiz olmalı; İngilizce
  bir kitapta "fi", "fl", "ff", "ffi" bitişik harfleri doğru görünmeli; harflerde kayma ya da çizgi
  görünmemeli.
- [ ] Okurken "%'ye git" penceresini ve Ayarlar'da bir aralık seçimi penceresini açın: büyük başlık
  (Noto Sans 18) düzgün olmalı. Ayarlar'daki yazı tipi önizlemesi, kitap Ayrıntılar ekranı ve bir
  sözlük tanımı da düzgün görünmeli.
- [ ] Geliştirme derlemesinin (`pio run -e x4pro`) seri günlüğünde `[FDC] Decompression failed`,
  `[FDC] Glyph … not found in any group`, `[FDC] Failed to allocate`, `page buffer slots full` ya
  da `Inflate state OOM` satırları görünmemeli.

## Bölüm dosyası tabloları Rust'ta ("rust: our own section file tables")

Kitap okunurken her bölümün sayfaları SD karttaki önbellekte bir bölüm dosyasına yazılır
(`sections/<n>.bin`). Bu dosyanın başlığını (yerleşim ayarlarının denetimi dahil), yarım kalmış bir
derlemenin ilerleme bilgisini ve sayfa, çapa (dipnot ve içindekiler hedefleri), paragraf, liste
öğesi ve görünür metin konumu tablolarını okuyan ve derleme sonunda yazan kod artık
`bembeyaz-section`'ın `file` modülünde; biçimin sürümü (60) ve geçmişi de orada. Dosya açma,
yeniden adlandırma, derlemenin yürütülmesi ve sayfa içerikleri C++'ta kaldı. 4eacfd40'a kadar
Section için hiç test yoktu; o commit'in 14 testi ve bu sürümün 5 regresyon testi gerçek HTML
ayrıştırıcısıyla bölüm kurup dosya biçimini ve her aramayı sabitliyor; toplam 969 host testi
geçiyor. Bir fark testi eski C++ kodunu Rust koduyla rastgele bölümlerde ve bozulmuş dosyalarda
karşılaştırdı: iki taraf bayt bayt aynı dosyayı yazdı. Okumadaki farkların hepsi bozuk dosyalardaki
bilinçli düzeltmelerdi; eski davranış tanımsız, cihazla bilgisayarda farklı ya da bir kilitlenme
olduğu için bu taşıma onları korumadı, ayrı ayrı regresyon testleriyle düzeltti:

- açık/kapalı ayarlarından birinin baytı 0 ya da 1 değilse dosya başka ayarlarla dizilmiş sayılır
  (eski kod bunu tanımsız davranışla bool olarak okuyordu);
- 4 GB'a yakın tablo konumları taşıp dosyanın başını göstermez;
- başlığın içini gösteren bir sayfa konumu sayfa diye okunmaz;
- içindekiler araması bozuk bir çapa adı uzunluğu yüzünden gigabaytlarca bellek ayırıp okumaz,
  orada durur (eski kod fark testinde dakikalarca takıldı).

Emülatörde okuma ve kitap açma ekranları değiştirilmemiş CrossPoint'le, okuma ekranları bir önceki
sürümle aynıydı; sayfa çizme süresi değişmedi (1.260–1.261 ms yerine 1.259–1.260 ms).

- [x] Önceki firmware'le açılmış bir kitabı (bir bölümü tam dizilmiş, uzun bir bölümü yarım
  kalmış) yeni sürümü yükledikten sonra açın: yeniden dizilmeden hemen açılmalı, günlükte
  `Unknown version` ya da `Parameters do not match` olmamalı.
- [x] Bir kitabı açıp birkaç sayfa okuyun, kitaptan çıkıp yeniden açın: aynı sayfada ve hemen
  açılmalı (bölüm önbellekten okunur).
- [ ] Okurken metin menüsünden yazı boyutunu değiştirin (Türkçe arayüzde bu satır şimdilik
  "Arayüz Yazı Boyutu" diye görünüyor): bölüm yeniden dizilmeli, sayfalar yeni boyutta olmalı;
  eski boyuta dönünce de doğru görünmeli.
- [ ] Uzun bir bölümün dizilmesi sürerken kitaptan çıkıp yeniden açın: kaldığınız sayfa hemen
  açılmalı; kayıtlı sayfaların sonuna yaklaşınca kalan sayfalar arkada dizilmeli ve bölümün sonuna
  kadar okunabilmeli.
- [ ] Aynı bölüm dosyasında birden çok içindekiler girdisi olan bir kitapta İçindekiler'den bir alt
  başlığa atlayın ve metindeki bir dipnot bağlantısını izleyin: ikisi de doğru yere gitmeli. Birkaç
  sayfa çevirip İçindekiler'i yeniden açın: imleç bulunduğunuz alt başlıkta olmalı.
- [ ] KOReader eşitlemesi kullanılıyorsa konumu "Yerel ilerlemeyi yükle" ile gönderin, başka bir
  bölüme geçin, eşitlemeyi yeniden açıp "Uzak ilerlemeyi uygula"yı seçin: gönderdiğiniz sayfaya
  dönmeli.
- [ ] Geliştirme derlemesinin (`pio run -e x4pro`) seri günlüğünde `[SCT] Deserialization failed`
  (ayar değiştirdikten sonra her bölüm ilk açıldığında görülen `Parameters do not match` dışında),
  `Failed to write the section tables` ya da `Failed to write LUT due to invalid page positions`
  satırları görünmemeli; uzun bölümden ilk kez çıkarken `Suspended build: … pages persisted`
  görünmeli.

## Sayfa bağlantılarının boş baytları sıfırlanır ("fix: zero page link hrefs in the section cache")

Bölüm dosyasındaki her sayfa, iç bağlantılarının hedefini 256 baytlık bir alanda saklar. Hedefin
sonundaki NUL'dan sonraki baytlar ilklendirilmiyordu: dosyaya o an bellekte ne varsa (başka
sözcüklerin metni, adresler) yazılıyordu; aynı kitap her dizilişte farklı bir dosya veriyordu. Artık
alan sıfırla ilklenir. Sayfa biçimini bayt bayt ve her doğrulama dalını sabitleyen 6 yeni host testi
(`test/page_codec`) dahil 975 host testi geçiyor; emülatörde okuma ekranları aynı.

- [x] İç bağlantıları (dipnot, içindekiler bağlantısı) olan bir bölümde bağlantılara dokunun
  (X4 Pro) ya da bağlantı seçimini kullanın: doğru yere gitmeli.

## Bölüm sayfaları Rust'ta ("rust: our own section pages")

Bölüm dosyasındaki her sayfanın gövdesini (satırlar ve sözcükleri, ruby, satır stili, sözcük
başına yazı tipleri; resimler, yatay çizgiler, büyük ilk harfler, çerçeve kutuları; dipnotlar ve
bağlantılar) okuyup doğrulayan ve yazan kod artık `bembeyaz-section`'ın `page` modülünde. Nesneleri
C++ kurar; arena, ruby, sözcük yazı tipleri, resim yolları ve dipnot/bağlantı dizileri için belleği
C++ ayırır; çizim C++'ta kaldı. Sayfa artık 256 baytlık bir tamponla okunuyor; eskiden her alan ayrı
bir `HalFile::read` çağrısıydı (arena ve sözcük yazı tipi tablosu zaten tek okumaydı). Eski
davranış korunuyor: dosyanın bittiği yerdeki bir alan sıfır olarak okunur. Bilinçli farklar:
satır stilindeki ya da çerçeve kutusundaki bir açık/kapalı baytı 0 ya da 1 değilse artık açık
sayılır (eski kod onu tanımsız davranışla bool olarak okuyordu); yazarken başarısız olan her yazma
artık sayfayı başarısız sayar (eski kod çoğu yazmanın sonucuna bakmıyordu). `test/page_codec`'in
b14b2167'deki 6 testine 5 yeni test (resim, kesik resim yolu, üç sözcük yazı tipi yuvası,
açık/kapalı baytı, art arda okunan bloklar) eklendi; 979 host testi geçiyor. Geliştirme sırasında
bir kerelik bir fark testi (ağaçta tutulmadı) 8.000 rastgele ve bozulmuş sayfada eski ve yeni kodun
çözüp geri yazdığı baytları karşılaştırdı: 56 sayfa dışında aynıydı, o 56 sayfadaki tek fark
açık/kapalı baytlarıydı. Emülatörde okuma, kitap açma, PNG ve JPEG resimli kitap ekranları
değiştirilmemiş CrossPoint'le, okuma ekranları bir önceki sürümle aynıydı; sayfa çizme süresi
(sayfanın SD karttan yüklenmesi hariç; yükleme ölçülmedi) değişmedi (1.260–1.261 ms yerine
1.257–1.258 ms).

- [ ] Bölüm biçimi değişmedi (sürüm 60): 089e7ae5 ya da sonrasıyla açılmış bir kitabı yeni sürümü
  yükledikten sonra açın: sayfalar yeniden dizilmeden, eskisi gibi görünmeli; seri günlükte
  `[SCT] Deserialization failed` ya da `Cache not found, building` olmamalı.
- [ ] Aşağıdaki adımlarda yeni yazıcının da çalışması için kitabın önbelleğini silin (Kitap
  Önbelleğini Sil) ya da hiç açılmamış bir kitapla başlayın; okuduktan sonra kitaptan çıkıp yeniden
  açın.
- [x] Resimli bir kitapta resim içeren sayfalar ve yer tutucuları doğru yerde olmalı.
- [x] Ayarlar'da Gömülü Stil açıkken, CSS'li bir kitapta (kalın, italik, altı/üstü çizili, üst ve
  alt simge, küçük büyük harf, farklı yazı boyutlu sözcükler; `::first-letter` ya da büyük ilk harf
  ile başlayan paragraflar; kenarlıklı ya da açık arka planlı kutular) sayfalar yeniden açınca aynı
  görünmeli. `<hr>` çizgileri Gömülü Stil kapalıyken de görünmeli.
- [ ] Odaklanmış Okuma açıkken Latin harfli bir kitapta sözcük başları yeniden açınca da kalın
  olmalı.
- [ ] Okuma yazı tipi olarak CJK destekli bir SD fontu seçiliyken furiganalı Japonca bir kitapta
  (ör. bir Aozora Bunko EPUB'u) ruby metinleri yeniden açınca yerinde olmalı.
- [x] Dipnot ve iç bağlantılı bir bölümde bağlantılar yeniden açınca da doğru yere gitmeli.
- [ ] Geliştirme derlemesinin (`pio run -e x4pro`) seri günlüğünde `[PGE] Deserialization failed`,
  `[PGE] Serialization failed`, `[TXB] OOM` ya da `[ERS] Failed to load page from SD` satırları
  görünmemeli.

## Kitap üst veri önbelleği Rust'ta ("rust: our own book metadata cache")

Bir kitap ilk açıldığında önbelleğe bir `book.bin` yazılır: başlık, yazar, dil, kapak, spine
öğeleri (bölüm dosyaları ve kümülatif boyutları) ve içindekiler. Bu dosyanın ve kurulduğu iki geçici
dosyanın biçimini okuyan ve yazan kod, ilerleme yüzdesinin dayandığı kümülatif boyut aramaları ve
içindekiler girdilerinin bölüm dosyalarına eşlenmesi artık `bembeyaz-book`'un `cache` modülünde.
Dosyalar ve tamponları, ZIP'teki boyutların bulunması, 400 öğe eşiği, dizinin sıralanması, kilit
ve NFC başlık birleştirme C++'ta kaldı. Okuma ve yazma, `bembeyaz-section` ile ortak yeni
`bembeyaz-io` crate'inin tamponlu okuyucu ve yazıcısıyla yapılıyor; kurulum eskisi gibi 4 KB'lık
tamponlarla, okumalar alan başına bir dosya okuması yerine küçük tamponlarla. 400 ya da daha çok
spine öğeli kitapların dizini artık iç RAM yerine tek parça PSRAM'de. Bu değişiklik test eklemiyor;
5618273f'nin 8 biçim testi dahil `test/book_metadata_cache`'in 13 testi geçiyor, toplam 987 host
testi. Geliştirme sırasında bir kerelik bir fark testi (ağaçta tutulmadı) son kodla 400 rastgele
kitabı (400 ya da daha çok spine öğesi olanlar, metin kitapları, NFD başlıklar dahil) eski ve yeni
kodla kurdu: `book.bin`'ler bayt bayt aynıydı; bozulmuş kopyalarda 10.578 okumanın biri dışında hepsi
aynıydı (ara sürümlerle 550 kitapta da öyle). O okumada bir boyut penceresinin son kaydı dosya
sonunda kesikti: eski kod onu bilgisayarda okunmuş
sayıyordu (dosya sonunu aşan bir konumlanmaya izin verildiği için); artık cihazdaki gibi okunmamış
sayılıyor. Geçici dosyalardaki bozuk bir kayıt artık kurulumu başarısız kılıyor (`book.bin` silinir);
eski kod kurulumda uzunluğu denetlemeden bellek ayırıyordu. Emülatörde okuma, kitap açma, PNG, JPEG
ve XML senaryolarının beşi de hem değiştirilmemiş CrossPoint'le hem bir önceki sürümle aynıydı; boş
bellek yaklaşık 600 bayt arttı.

- [ ] 5f321ebd ya da sonrasıyla açılmış bir kitap yeni sürümde açılmalı (sürüm 11 değişmedi):
  başlık, yazar ve kapak aynı görünmeli; seri günlükte `[BMC] Loaded cache data` görünmeli,
  `[EBP] Cache not found, building` ya da `[BMC] Cache version mismatch` görünmemeli.
- [ ] Bir kitabın önbelleğini silin (Kitap Önbelleğini Sil) ve açın: içindekiler ve bölüm adları
  doğru olmalı; okuma menüsünde "%'ye git" ile %50'ye gidin: durum çubuğundaki yüzde yaklaşık %50
  göstermeli, varılan bölüm İçindekiler'de kitabın ortası civarında olmalı.
- [ ] 400 ya da daha çok bölüm dosyası olan bir kitabı (bilgisayarda
  `unzip -p kitap.epub '*.opf' | grep -c '<itemref'` ile sayılır) ilk kez açın: günlükte
  `[BMC] Using fast index` ve `Using batch size lookup` görünmeli, içindekiler bölümlere doğru
  eşlenmeli; `[EBP] Total indexing completed in … ms` önceki firmware'dekinden belirgin uzun
  olmamalı.
- [ ] Bir TXT ve bir Markdown kitabını ilk kez açın, okudukça ilerleme yüzdesi artmalı; kapatıp
  yeniden açınca günlükte `[EBP] TXT/MD cache invalid or outdated` görünmemeli.
- [ ] Geliştirme derlemesinin (`pio run -e x4pro`) seri günlüğünde `[BMC] Cache file damaged`,
  `[BMC] Failed building book.bin`, `[BMC] Could not index`, `[BMC] Could not read the spine` ya
  da `[BMC] Failed writing` satırları görünmemeli.

## Alıntı deposu Rust'ta ("rust: our own clipping store")

Okurken kaydedilen alıntılar her kitap için kartta bir dosyada tutulur
(`/.crosspoint/clippings/epub_….bin`): kitabın başlığı ve yazarı, her alıntının yeri (bölüm, sayfa,
sözcükler, kaynak konumu), eşitleme durumu ve metni. Bu dosyayı okuyan ve yazan kod ile silinen
alıntıların eşitleme için bekleyen kimlik günlüğünü (`….bin.deleted`) tarayan kod artık
`bembeyaz-clippings`'te. Dosyaların açılması, yedek ve geçici dosyayla güvenli değiştirme,
bellekteki alıntılar, günlüğe yazma ve kimlik üretme C++'ta kaldı. Eski 1., 2. ve 3. sürüm dosyalar
eskisi gibi okunur ve ilk yazmada 4. sürüme çevrilir. Yazma artık alan başına bir dosya yazması
yerine 128 baytlık bir tamponla, okuma ise kayıt başına aşağı yukarı tek dosya okumasıyla yapılıyor.
Artık kullanılmayan `Serialization.h` silindi. Bir önceki commit dosya biçimini 7 testle sabitledi
(bağımsız bir çözücüyle 4. sürüm, elle kurulmuş 1., 2. ve 3. sürüm dosyalar, bozuk dosyalar, silme
günlüğü); toplam 1001 host testi geçiyor. Geliştirme sırasında bir kerelik bir fark testi (ağaçta
tutulmadı) eski ve yeni kodu 12.000 rastgele durumda (her birinde bir alıntı dosyası ve bir silme
günlüğü) çalıştırdı (geçerli ve bozuk dosyalar, yükleme, eşitleme, silme, uzaktan gelen
değişiklikler, yazma hataları; yaklaşık 30.000 işlem ve 33.000 günlük okuması): her çıktı ve her
yazılan dosya bayt bayt aynıydı; koda bilerek konan bir hata ilk durumlarda yakalandı. Yeni
`emu/clipping.txt` senaryosu bir sözcüğü alıntılıyor, kitabı yeniden açıyor (vurgu geri geliyor),
alıntıyı uzun basarak kaldırıyor ve kitabı bir daha açıyor; bu ve okuma, kitap açma, PNG, JPEG ve
XML senaryoları bir önceki sürümle aynı ekranları ve aynı boş belleği verdi. x4pro'da flash 372 bayt
küçüldü, RAM değişmedi.

- [ ] Önceki firmware'le kaydedilmiş alıntıları olan bir kitabı açın: vurgular aynı yerlerde
  görünmeli; okuma menüsündeki "Alıntıları Görüntüle" listesinde her alıntının bölümü ve metin önizlemesi
  doğru olmalı, birini açınca tam metni görünmeli.
- [ ] Bir sözcüğe uzun basıp Clip ile yeni bir alıntı kaydedin, kitabı kapatıp yeniden açın: vurgu
  kalmalı, listede görünmeli. Vurgunun üstüne uzun basıp kaldırın, yeniden açınca gitmiş olmalı;
  diğer alıntıların metinleri bozulmamış olmalı.
- [ ] Alıntı eşitlemesi kuruluysa: bir alıntı ekleyip eşitleyin, sonra eşitlenmiş bir alıntıyı
  silip yeniden eşitleyin: silme sunucuya ulaşmalı ve sonraki eşitlemede alıntı geri gelmemeli.
- [ ] Geliştirme derlemesinin (`pio run -e x4pro`) seri günlüğünde `[CLIP] Clipping file damaged
  or cut`, `[CLIP] Failed to write clipping file` ya da `[CLIP] Invalid or unreadable clipping
  deletion journal` satırları görünmemeli.

## Alıntı metinleri tek seferde kopyalanır ("perf: copy clipping texts in one read and one write")

Bir alıntı eklendiğinde ya da silindiğinde alıntı dosyası baştan yazılır ve eski metinler eski
dosyadan kopyalanır. Kopyalama 128 baytlık parçalarla yapılıyordu: SdFat'ın tek sektör önbelleği her
parçada iki dosya arasında gidip geldiği için büyük bir depo çok yavaştı. Artık her metin, dosya her
yeniden yazıldığında PSRAM'den ayrılan 4 KB'lık bir tamponla (en uzun alıntı metni kadar) tek okuma
ve tek yazmayla kopyalanıyor; tampon yalnızca yazma süresince ve yalnızca eski dosya varken
ayrılıyor. SD sürücüsü kendi DMA tamponunu kullandığı için PSRAM sorun değil. PSRAM'de yer yoksa
kopya eskisi gibi 128 baytlık parçalarla yapılır: daha yavaş, ama yazma başarısız olmaz (yeni bir
host testi bunu sabitliyor; toplam 1003 test). Yeni `emu/make_clipping_store.py` 1000 adet 4 KB'lık
alıntılı (yaklaşık 4 MB) bir depo kuruyor; `emu/clipping-large.txt` bu depoyla bir alıntı ekleyip
siliyor. Emülatörde ekleme 90,4 s'den 11,1 s'ye, silme 90,9 s'den 11,3 s'ye indi (yaklaşık 8 kat);
ekran görüntüleri ve boş bellek değişmedi, ekleyip silen çalıştırmada iki firmware'in kartta
bıraktığı dosyalar bayt bayt aynıydı. Eski ve yeni kodun 12.000 durumluk fark testinde çıktılar yine
bayt bayt aynıydı. Kalan 11 saniyenin çoğu, hizasız metinlerin sektör kenarlarındaki tek sektörlük
SD komutları; daha fazlası için dosyanın değişmeyen kısımları sektör hizalı bloklarla kopyalanmalı
(ayrı bir iş). x4pro'da flash 152 bayt büyüdü, RAM değişmedi.

- [ ] Çok alıntılı bir kitapta (onlarca uzun alıntı) bir alıntı ekleyin ve silin; önceki firmware'le
  ve bununla süreleri not edin (emülatörde 4 MB'lık depoda yaklaşık 8 kat hızlandı). Sonra kitabı
  yeniden açın: "Alıntıları Görüntüle" listesindeki diğer alıntıların metinleri doğru olmalı.
- [ ] Alıntı eşitlemesi kuruluysa alıntıları olan bir kitabı eşitleyin: alıntılar eskisi gibi
  gidip gelmeli, seri günlükte `[CLIP] Failed to write clipping file` görünmemeli.

## Alıntı dosyası tam bloklarla yazılır ("perf: write the clipping store in whole buffers")

Alıntı dosyası yeniden yazılırken geçici dosyaya giden yazmalar artık 8 KB'lık bir
`BufferedFileWriter` tamponundan geçiyor (framework'ün 4 KB'lık iç RAM sınırının üstünde olduğu için
malloc önce PSRAM'i dener; tampon hiç ayrılamazsa eskisi gibi doğrudan yazılır, yeni bir host testi
bunu da sınıyor). `BufferedFileWriter` artık tamponu yalnızca dolunca boşaltıyor: son yazma dışında
her yazma tam 8 KB ve sektör hizalı, SdFat bunları sektör önbelleğini kullanmadan doğrudan karta
yazıyor; önbellek eski dosyadan okumaya kalıyor. Dosyaya yazılan baytlar aynı; aynı sınıfı kullanan
kitap önbelleği (`book.bin` kurulumu) ve kütüphane dizini de yazmalarını artık tam tampon boyunda
yapıyor. Emülatörde 4 MB'lık depoda ekleme 11,0 s'den 7,7 s'ye, silme 11,3 s'den 8,0 s'ye indi;
bunun ~2 s'si dokunma ve sayfa çizimi, kalan ~6 s aşağı yukarı 4 MB okuyup 4 MB yazmanın süresi.
Aynı 8 KB'lık tampon eski boşaltma kuralıyla neredeyse hiç kazandırmadı (10,8 s ve 11,1 s): kazancı
hizalı yazma getiriyor. Eski dosyayı da hizalı okumak ~0,5 s daha kazandırırdı; eklenmedi. İki
firmware'in kartta bıraktığı alıntı dosyası ve `book.bin` bayt bayt aynıydı; ekran görüntüleri aynı;
alıntı deposunun fark testinde çıktılar yine aynı. Toplam 1006 host testi geçiyor. x4pro'da flash
100 bayt büyüdü, RAM değişmedi.

- [ ] Çok alıntılı bir kitapta bir alıntı ekleyip silin; önceki firmware'le ve bununla süreleri not
  edin (emülatörde 4 MB'lık depoda ekleme ve silme ~%30 kısaldı). Kitabı yeniden açınca diğer
  alıntıların metinleri doğru görünmeli.
- [ ] Bir kitabı açıp okuma menüsünden Kitap Önbelleğini Sil'i seçin; ana ekrana dönünce kitabı
  yeniden açın: içindekiler ve bölümler doğru olmalı. Dosya Transferi (web) ile karta yeni bir kitap
  yükleyip kütüphaneyi açın (ya da kütüphanenin "Recent" sekmesinde bir kitaba uzun basıp "Refresh
  library"yi seçin): yeni kitap görünmeli. Seri günlükte `[BMC] Failed building book.bin`, `[BMC] Failed writing` ya
  da `[ERR] [LIBIDX]` satırı görünmemeli.

## Kütüphane dizini yazıcısı Rust'ta ("rust: our own library index writer")

Kütüphane dizini (`/.crosspoint/library.idx`) kurulurken kayıtların, iki sıralama permütasyonunun
(yazar ve geliş) ve ad bloblarının (yol özeti, dosya adı, gösterilen yazar, başlık, kaynak yazar)
yazılması; başlık, yazar anahtarı ve gösterilen yazımın soyadı sıralama anahtarlarının aşama
dosyasından okunması; aynı kişinin farklı yazımları arasından en çok kullanılanın seçilmesi
(eşitlikte kısa olan, sonra alfabetik ilk) ve değişiklik zamanlarının okunması artık
`bembeyaz-library`'nin `build` modülünde. Klasör gezintisi, kitapların üst verisini okumak, aşama
dosyası, eski dizinle eşleştirme (yeniden adlandırılan kitapların geliş sırasını koruma), bellek
ayırma ve bozulma kararları, sıralama (`std::sort`), başlık ve klasör bölümü ile dosyaların yerine
konması C++'ta kaldı. Bir önceki commit (9421d9ca) dizini 3 testle sabitledi: dizinin her bölümünü
bağımsız çözen bir denetleyici, üretilmiş kartlarda (ilk kurulum; silme, yeniden adlandırma, ekleme
ve dokunmadan sonra; üst veri olmadan) ve her bellek ayırma hatası noktasında dizinin kaydedilmiş
özetleri, elle denetlenen bir yazım oylaması; toplam 1009 host testi geçiyor. Geliştirme sırasında
bir kerelik bir fark testi (ağaçta tutulmadı) eski ve yeni kodu 1200 rastgele kartta çalıştırdı
(2957 kurulum; 688'inde bellek, okuma, yazma ya da yeniden adlandırma hatası enjekte edildi): her
sonuç, istatistik ve dizin bayt bayt aynıydı; koda bilerek konan bir hata hemen yakalandı. Yeni
`emu/library.txt` senaryosu kütüphaneyi açıp dizini kuruyor ve bir daha açıyor; `emu/books` kartında
(15 kitap) ve 600 TXT dosyalı bir kartta bir önceki sürümle ekranlar aynı, dizinler değişiklik
zamanları dışında bayt bayt aynıydı (kartlar farklı anlarda hazırlandığı için zamanlar 36–38
saniyelik bir farkla kayıktı), kurulum süresi de neredeyse aynı kaldı (600 kitapta yazma aşaması
eski 4738, yeni 4697 ms). x4pro'da flash 1232 bayt büyüdü (Rust kodu yerine geçtiği C++'tan biraz
büyük), RAM değişmedi.

- [ ] Önceki firmware'le kurulmuş bir kütüphaneyi açın: kitaplar, "Title", "Author" ve "Recent"
  sekmelerindeki sıralar ve yazar adları eskisi gibi görünmeli. Sonra karta bir kitap ekleyin
  (Dosya Transferi ile) ve kütüphaneyi açın: dizin yeni firmware'le yeniden kurulur, sıralar ve
  yazar adları yine eskisi gibi olmalı, yeni kitap görünmeli.
- [ ] Aynı yazarın adı üst verisinde farklı yazılmış birkaç EPUB'u (örneğin "Lu Xun" ve "Xun Lu") karta koyup
  kütüphanenin "Recent" sekmesinde bir kitaba uzun basarak "Refresh library"yi seçin: hepsi aynı
  yazarın altında, en çok kullanılan yazımla görünmeli; "Author" sekmesinde soyadına göre
  dizilmeli; yazarı olmayan kitaplar sonda, "Unknown author" altında kalmalı.
- [ ] Yüzlerce kitaplı bir kartta bir kitap ekleyin ya da silin, sonra kütüphaneyi yenileyin
  ("Recent" sekmesinde uzun basıp "Refresh library"): süre önceki firmware'dekine yakın olmalı,
  seri günlükte `[LIBIDX] built:` görünmeli; `[ERR] [LIBIDX]` satırı görünmemeli. (Kartta hiçbir
  şey değişmediyse dizin yeniden yazılmaz ve günlükte `[LIBIDX] unchanged:` görünür.)

## Bölüm metni Rust'ta sözcüklere bölünür ("rust: our own chapter word cutting")

Bir bölüm açılırken metin sözcüklere bölünür. Bu işi yapan bayt döngüsü (ChapterHtmlSlimParser'ın
`characterData`'sı) artık `bembeyaz-html`'in `words` modülünde: boşluk sözcüğü bitirir; kırılmaz
boşluk (U+00A0, U+202F) iki yanındaki sözcüğe bitişik bir boşluk sözcüğü olur; BOM atılır; 200
baytlık tamponu dolduran sözcük kod noktası sınırında kesilir ve devamı bitişik gelir; `white-space:
pre` her boşluğu sözcük olarak tutar, sekmeyi dört boşluğa, satır sonunu satır kırmaya çevirir; her
sözcük ilk baytının görünür konumuyla eklenir. Sözcük tamponu, sözcüğün paragrafa eklenmesi, satır
kırma ve büyük ilk harf yakalama C++'ta kaldı; Rust bunlara geri çağrıyla ulaşır. Bir önceki commit
(f559aecd) bölmeyi 4 testle sabitledi; toplam 1013 host testi geçiyor. Geliştirme sırasında bir
kerelik bir fark testi (ağaçta tutulmadı) eski ve yeni ayrıştırıcıyı 3000 rastgele bölümde
çalıştırdı (rastgele etiketler, satır içi stiller, büyük ilk harf kuralları, tablolar, metnin
rastgele baytlarda, UTF-8 dizilerinin ortasında bölünmesi): ortaya çıkan her sayfa, bölüm
önbelleğinde saklandığı biçimiyle, bayt bayt aynıydı; önce eski koddaki sekiz dala (sekme, iki
kırılmaz boşluk, BOM, UTF-8 kesmesi, büyük ilk harf iptali, dipnot bağlantısı, `pre` satırı) bilerek
konan hataların hepsini yakaladığı, sonra yeni koda konan bir hatayı da yakaladığı doğrulandı.
Emülatörde okuma, alıntı, XML ve PNG senaryoları ile iki yönlü, CSS, heceleme ve XML kitaplarının
açılışı bir önceki sürümle aynı ekranları verdi; dört kitabın kartta oluşan bölüm önbelleği bayt
bayt aynıydı, bölüm kurma süreleri de aynı kaldı (±1 ms). x4pro'da flash 616 bayt büyüdü, RAM
değişmedi.

- [ ] Birkaç kitabın önbelleğini silip açın (okuma menüsünden Kitap Önbelleğini Sil): metin, satır
  kırılmaları ve sayfalar önceki firmware'deki gibi olmalı.
- [ ] Kırılmaz boşluk içeren bir kitap açın (Fransızca kitaplar genelde « » ile ; : ! ? işaretlerinden
  önce kullanır): kırılmaz boşluğun iki yanı aynı satırda kalmalı.
- [ ] Boşluksuz uzun metin içeren bir bölüm açın (örneğin Çince ya da Japonca): metin eksiksiz
  görünmeli, karakterler bozulmamalı.
- [ ] `<pre>` kod bloğu ya da `white-space: pre` kullanan bir bölüm açın (sekmeli olursa daha iyi):
  boşluklar, sekmeler ve satırlar korunmalı.
- [x] CSS'inde `::first-letter` ya da `initial-letter` olan (ya da büyük ilk harf span'ı kullanan) bir
  kitap açın: büyük ilk harf eskisi gibi görünmeli.

## Büyük ilk harfin yakalanması Rust'ta ("rust: our own initial letter capture")

Büyük ilk harfin (drop cap) metninin yakalanması da artık `bembeyaz-html`'in `words` modülünde:
büyük harf span'ı kendi metnini (en çok üç kod noktası ve 12 bayt) alır; `::first-letter` kuralı
paragrafın baştaki boşluklarını atlar, baştaki noktalamayı ve ilk harfi alır; daha uzun olan ya da
(span'da) boşlukla karşılaşan yakalama bırakılır ve metni sözcüğe geri döner. Yakalama durumu
C++'taki `DropCapState` ile aynı yerleşimdeki `bb_html_initial_letter`'da tutuluyor; büyük harfin
yerleşimi ve çizimi C++'ta kaldı. Bir önceki commit (df2b06b4) yakalamayı 4 testle sabitledi; toplam
1017 host testi geçiyor. Bölüm ayrıştırıcısının fark testi 3000 rastgele bölümde, metin XML
ayrıştırıcısının yaptığı gibi tam karakterlerle bölündüğünde, eski ve yeni sayfaları bayt bayt aynı
buldu; koda bilerek konan bir hata yakalandı. Metin yapay olarak UTF-8 dizilerinin ortasından
bölündüğünde 3000 durumun 10'unda fark çıktı: eski kod yakalanan karakterin noktalama olup
olmadığına karar verirken parçanın sonundan ötesini okuyordu, yeni kod okumuyor; eski kod yalnızca
parça içinden okuyacak şekilde değiştirilince bu farkların hepsi kayboldu. XML ayrıştırıcısı metni
hiçbir zaman karakter ortasından bölmediği için bu fark kitaplarda ortaya çıkmaz. Emülatörde okuma
senaryosu ve büyük ilk harfli CSS kitabı ile iki yönlü ve heceleme kitaplarının açılışı aynı
ekranları verdi, CSS kitabının bölüm önbelleği bayt bayt aynıydı. x4pro'da flash 208 bayt
büyüdü, RAM değişmedi.

- [x] CSS'inde `::first-letter` ya da `initial-letter` olan (ya da büyük ilk harf span'ı kullanan) bir
  kitap açın: büyük ilk harf ve önündeki tırnak ya da noktalama eskisi gibi görünmeli; paragrafın
  geri kalanı harfin yanında akmalı. (Böyle bir kitap `python3 emu/make_css_epub.py <klasör>` ile
  üretilebilir: `p.drop::first-letter` kuralı içerir.)

## Satır içi stil yığını Rust'ta ("rust: our own inline style stack")

Bir bölüm ayrıştırılırken metin stili belirleyen her açık öğe (bloklar, kalın, italik, altı ya da
üstü çizili, dipnot bağlantısı, üst ve alt simge, stilli span, `small`/`big`) yığına bir stil
girdisi koyar; sözcüklerin stili bu girdilerin bloğun stili üzerine sırayla birleştirilmesiyle
bulunur: çizgiler birikir ve bir çocuk atalarının çizgisini kaldıramaz, üst ve alt simge birbirinin
yerini alır, kalınlık ve italiklik miras alınıp değiştirilebilir. Girdilerin kurulması ve
birleştirilmesi artık `bembeyaz-css`'in `inline` modülünde; yığın C++'ta, Rust'la ortak
`bb_css_inline_entry` girdileriyle tutuluyor. Bir önceki commit (f960d2c9) birleştirmeyi 4 testle
sabitledi; toplam 1021 host testi geçiyor. Bölüm ayrıştırıcısının fark testi, etiket havuzuna `u`,
`s`, `strike`, `del`, `big`, `ins` ve yön, hizalama, dekorasyon ve kalınlık stilleri eklenerek 4000
rastgele bölümde eski ve yeni sayfaları bayt bayt aynı buldu; koda bilerek konan bir hata yakalandı
("üst simge alt simgeyi siler" kuralını kaldıran ikinci bir hata görünür bir fark yaratmadı: sözcük
stilinde üst simge zaten alt simgeye üstün gelir). Emülatörde okuma ve alıntı senaryoları ile CSS,
iki yönlü, heceleme ve XML kitaplarının açılışı bir önceki sürümle aynı ekranları verdi. x4pro'da
flash 568 bayt büyüdü (C++ nesnesi 1073 bayt küçüldü), RAM değişmedi.

- [ ] Kalın, italik, altı çizili ve üstü çizili metin, üst ve alt simgeler (dipnot numaraları), küçük
  büyük harf ve sağdan sola metin içeren kitapları açın (`python3 emu/make_css_epub.py <klasör>` bir
  kısmını üretir): bu stiller eskisi gibi görünmeli.

## Satır içi yazı boyutu Rust'ta ("rust: our own inline font scale")

Bir bölüm ayrıştırılırken her bloğun yazı boyutu şöyle bulunur: ebeveyn bloğun boyutu açık satır içi
öğelerin boyutlarıyla çarpılır (rem olan bir öğe değeri baştan koyar), üstüne bloğun CSS
`font-size`'ı (rem ise doğrudan, değilse ebeveyne göre) ya da başlıkların h1–h6 varsayılan oranları
uygulanır; sonuç 0,5 ile 3 arasına sıkıştırılır. Satır içi `font-size` taşıyan span'lardaki
sözcüklerin boyutu da yığındaki boyutların bloğun boyutuyla aynı biçimde çarpılmasıyla bulunur. Bu
iki hesap artık `bembeyaz-css`'in `inline` modülünde; boyutlu yazı tipinin seçilmesi ve paragraftaki
boyut yuvaları C++'ta kaldı. Bir önceki commit (a360179b) başlıkların ve satır içi boyutların tam
değerlerini 3 testle sabitledi; toplam 1024 host testi geçiyor. Bölüm ayrıştırıcısının fark testi,
rastgele bölümlerin dörtte üçüne yapay boyutlu yazı tipleri ekleyerek 4000 rastgele bölümde eski ve
yeni sayfaları bayt bayt aynı buldu; başlık oranlarına bilerek konan bir hata yakalandı (sıfır
boyutlu bir öğenin ardından gelenin yeniden paragrafın boyutundan başlamasını bozan ikinci bir hata
fark testinde görünmedi, onu Rust birim testi yakalıyor). Emülatörde okuma ve kitap açma senaryoları
CSS, iki yönlü, heceleme ve XML kitaplarında bir önceki sürümle aynı ekranları verdi. Bu iki
fonksiyon `float`'ı FFI'dan değer olarak geçiren ilk fonksiyonlar; firmware'in dökümünde GCC'nin ve
Rust'ın argümanı ve dönüşü aynı yazmaçlarda taşıdığı görüldü (yanlış olsaydı başlıkların ve boyutlu
span'ların boyutu bozulurdu). x4pro'da flash 240 bayt büyüdü, RAM değişmedi.

- [x] Başlıkları (h1–h6), `font-size` verilmiş paragrafları ve yazı boyutu büyütülmüş ya da
  küçültülmüş span'ları (`small`/`big` ya da `font-size` verilmiş) olan bir kitabı açın (`python3
  emu/make_css_epub.py <klasör>` bir kısmını üretir): yazı boyutları eskisi gibi görünmeli.

## Bloğu tutan span'ın boyutu Rust'ta ("rust: our own block span font scale")

Bir paragrafı açıp tüm metnini tutan `font-size`'lı span (ör. `<p><span style="font-size:
2em">Birinci Bölüm</span></p>`) bloğun tamamını boyutlandırır: span'ın boyutu paragrafın ya da onu
saran böyle span'ların boyutunun katı (rem ise doğrudan) olarak bulunup 0,5 ile 3 arasına
sıkıştırılır. Bu hesap ve `small` (0,83) ile `big`'in (1,2) varsayılan oranları artık
`bembeyaz-css`'in `inline` modülünde, başlıkların oranlarının yanında; span'ın bloğu tutup
tutmadığının izlenmesi C++'ta kaldı. Bir önceki commit (aeab5b62) span'ın boyutunu 1 testle
sabitledi; toplam 1025 host testi geçiyor. Bölüm ayrıştırıcısının fark testi 5000 rastgele bölümde
eski ve yeni sayfaları bayt bayt aynı buldu; koda bilerek konan üç hata (üst sınırın 2,9'a inmesi,
`small`'ın 0,84 olması, rem boyutun da çarpılması) yakalandı. Emülatörde okuma ve kitap açma
senaryoları CSS, iki yönlü, heceleme ve XML kitaplarında bir önceki sürümle aynı ekranları verdi.
`bb_css_span_scale`'ın argüman ve dönüş değerindeki `float` geçişi firmware'in dökümünde yine
doğrulandı. x4pro'da flash 160 bayt büyüdü, RAM değişmedi.

- [ ] Bölüm başlığı bir paragrafın içinde büyük yazılı bir span olan (ör. `<p><span
  style="font-size: 2em">Birinci Bölüm</span></p>`) ve `small`/`big` kullanan bir kitabı açın:
  başlık ve bu metinler eskisi gibi boyutlanmalı.

## Blok stili Rust'ta ("rust: our own block style")

Her blok öğesinin (paragraf, başlık, `div`, alıntı, liste öğesi…) yerleşim stili CSS'inden kurulur:
üst, alt ve yan kenar boşlukları, iç boşluklar ve ilk satır girintisi em, rem, punto, yüzde ya da
pikselden sıfıra doğru yuvarlanarak tam piksele çevrilir, her yan boşluk 2em ile sınırlanır, okurun
hizalama ayarı ("Kitabın Stili" seçili değilse) kitabınkinin önüne geçer, sayfa sonları ve yön
alınır; iç içe bloklarda yan boşluklar toplanır, üst ve alt kenar boşlukları büyük olana çöker, üst
ve alt iç boşluklar toplanır, ebeveynin önündeki sayfa sonu çocuğa geçer, girinti, hizalama, yön ve
yazı boyutu tanımlamayan çocuğa ebeveynden geçer. Bu hesaplar artık `bembeyaz-css`'in `block`
modülünde; `BlockStyle` yapısı C++'ta kaldı, Rust onu yerleşimi iki tarafta denetlenen
`bb_css_block` olarak okuyup yazıyor; Rust'a giden üç gövde başlıktan yeni `BlockStyle.cpp`'ye
taşındı. Bir önceki commit (fbbd39e4) davranışı 4 testle sabitledi, bu commit boş bir stilin
varsayılan bloğu verdiğini denetleyen bir test ekledi; toplam 1030 host testi geçiyor. Bölüm
ayrıştırıcısının fark testi, rastgele stillere büyük yan boşluklar, punto ve yüzde değerleri,
negatif girinti ve sayfa sonu eklenerek 5000 rastgele bölümde eski ve yeni sayfaları bayt bayt aynı
buldu; koda bilerek konan sekiz hatanın (yan boşluk sınırı, punto ve yüzde çevirisi, girinti,
girintinin ve sayfa sonunun ebeveynden geçmesi, üst kenar boşluğunun çökmesi, bloktan sonraki sayfa
sonu) hepsi yakalandı; yan boşluk sınırındaki hata ancak bu stiller eklenince görüldü. Emülatörde
okuma ve kitap açma senaryoları CSS, iki yönlü, heceleme ve XML kitaplarında bir önceki sürümle aynı
ekranları verdi. x4pro'da flash 544 bayt büyüdü (C++ ayrıştırıcısı 585 bayt küçüldü), RAM değişmedi.

- [ ] Kenar boşlukları, girintileri, ortalanmış ya da sağa yaslı paragrafları, alıntıları, iç içe
  listeleri ve sayfa sonları olan bir kitabı açın (`python3 emu/make_css_epub.py <klasör>` bir kısmını
  üretir); **Paragraf hizalaması** ayarını "Kitabın Stili" ile başka bir değer arasında değiştirip
  deneyin: boşluklar, girintiler ve hizalama eskisi gibi görünmeli.

## Öğe sınıflandırması Rust'ta ("rust: our own chapter element classification")

Bölüm ayrıştırıcısı her açılan ve kapanan öğenin adından ne olduğuna karar verir: `h1`–`h6` başlık
düzeyi, `p`, `li`, `div`, `br`, `blockquote`, `ul`, `ol`, `pre`, `figure` blok, `b`/`strong` kalın,
`i`/`em` italik, `u`/`ins` altı çizili, `del`/`s`/`strike` üstü çizili, `img`/`image` resim,
`table`/`tr`/`td`/`th` tablo sayılır; bu adlar büyük/küçük harfe duyarlı karşılaştırılır. Adı büyük
ya da küçük harfle yazılsın `head`, `style`, `script`, `title` ve `rp` öğelerinin metni gösterilmez.
`colspan` ve `rowspan` yalnızca rakamlardan oluşursa okunur, değilse 1'dir (0 ya da taşan değer
tablonun geri kalanı demektir); bir bağlantı da `http://`, `https://`, `mailto:`, `ftp://`, `tel:`
ya da `javascript:` ile başlamıyorsa kitabın içinde sayılır. Bu kararlar artık `bembeyaz-html`'in
`element` modülünde. C++ açılışta ve kapanışta sınıflandırmayı birer kez çağırıp bayraklara bakıyor
(eskiden etiket listelerini `strcmp` ile tek tek tarıyordu); `a`, `hr`, `span`, `sup`/`sub`,
`ruby`/`rt` gibi tek tek denetlenen etiketler C++'ta kaldı; `VisibleTextUtils.h` silindi. Bir önceki
commit (9c1b95ec) davranışı 4 testle sabitledi; toplam 1034 host testi geçiyor. Bölüm
ayrıştırıcısının fark testi, etiket havuzuna `rp`, `title`, `th`, `figure`, `ol`, `image` ve büyük
harfli `STRONG`, `B` eklenerek 5000 rastgele bölümde eski ve yeni sayfaları bayt bayt aynı buldu;
koda bilerek konan yedi hatadan beşi (sınıflandırmadakiler) yakalandı, bağlantı denetimine konan
ikisi rastgele bölümlerde görünmedi, onları sabitleme testi ve Rust birim testi yakalıyor.
Emülatörde okuma ve kitap açma senaryoları CSS, iki yönlü, heceleme ve XML kitaplarında, JPEG ve PNG
senaryoları da resimli kitaplarda bir önceki sürümle aynı ekranları verdi. x4pro'da flash 24 bayt
büyüdü, RAM değişmedi.

- [ ] Başlıklar, kalın, italik, altı ve üstü çizili metin, resimler, tablolar (`colspan`'lı hücreler
  dahil), ruby (furigana) metni, dipnotlar ve dış bağlantılar içeren bir kitabı açın: hepsi eskisi
  gibi görünmeli; ruby'nin ayraçları (`rp`) görünmemeli, dipnot bağlantıları açılmalı, dış
  bağlantılar kitabın içinde bir yere gitmemeli.

## Sayfa sonunda taşınan satırlar Rust'ta ("rust: our own page break carry")

Bölüm ayrıştırıcısı bir sayfayı kırarken son satırlarından bazılarını sonraki sayfaya taşır: bir
paragrafın sayfa altında yalnız kalacak ilk satırı, sonraki sayfayı tek başına açacak son satırı
yanındaki bir satırla (paragrafın sayfadaki iki satırıysa ikisi birden) ve paragrafın başı
taşınıyorsa hemen üstündeki başlığın satırları; taşınan satırlar sayfanın sonundaki, ayrıştırıcının
hatırladığı en çok altı metin satırından olabilir ve sayfa hiçbir zaman büsbütün boşalmaz. Bir resim
ya da büyük ilk harfli (drop cap) bir paragraf da, sığmadığında, üstündeki başlığı yanına alabilir;
büyük ilk harfli paragrafın kendi satırları taşınmaz. Bu kararlar artık `bembeyaz-layout`'un `carry`
modülünde (ABI'si yeni `carry.h`); sayfanın son öğelerinden kaçının taşınabileceğini bulmak,
satırları ve bağlantılarını taşımak ve son satırların konumlarını tutmak C++'ta kaldı. Bir önceki
commit (9cd2d95b) kararları 2 testle sabitledi; toplam 1036 host testi geçiyor. Bölüm
ayrıştırıcısının fark testi 5000 rastgele bölümde eski ve yeni sayfaları bayt bayt aynı buldu; koda
bilerek konan beş hatanın hepsi yakalandı. Emülatörde okuma ve kitap açma senaryoları CSS, iki
yönlü, heceleme ve XML kitaplarında, JPEG senaryosu da resimli kitapta bir önceki sürümle aynı
ekranları verdi. x4pro'da flash 216 bayt büyüdü, RAM değişmedi.

- [ ] Başlıkları ve uzun paragrafları olan bir kitapta birkaç sayfa ilerleyin: paragrafların sayfa
  altında ya da başında yalnız kalan tek satırları ve sayfa altındaki başlıklar eskisi gibi sonraki
  sayfaya taşınmalı.

## Tablo satırı geometrisi Rust'ta ("rust: our own table row geometry")

Bölüm ayrıştırıcısı bir tablo satırını ızgara olarak dizerken her sütunun genişliğini (ön taramayla
planlanmışsa onu, değilse sayfa genişliğinin eşit payını), sol kenarını (sağdan sola bir satırda
sütunlar aynalanır), hücre çerçevesinin genişliğini (yandaki sütuna bir piksel taşar, sayfa
kenarında durur) ve satırın ayırdığı en büyük üst ve alt iç boşluğu bulur; satır tek hücreliyse, bir
hücrenin yan boşlukları sütununu dolduruyorsa ya da planlanmamış bir sütun üç satır yüksekliğinden
darsa hücreler alt alta dizilir. Bu hesap artık `bembeyaz-layout`'un `table` modülünde, sütun
genişliklerinin planının yanında; hücreleri dizmek, sayfayı kırmak ve çerçeveleri çizmek C++'ta
kaldı. Bir önceki commit (f17b4572) satır geometrisini 6 testle sabitledi; toplam 1042 host testi
geçiyor. Bölüm ayrıştırıcısının fark testi, rastgele bölümlere çerçeveli ya da çerçevesiz, sağdan
sola satırlı, farklı iç boşluklu hücrelerden oluşan düzgün tablolar ve 120 ile 481 piksel arası
sayfa genişlikleri eklenerek 5000 rastgele bölümde eski ve yeni sayfaları bayt bayt aynı buldu; koda
bilerek konan beş hatanın hepsi yakalandı (bu tablolar eklenmeden önce hiçbiri görünmüyordu).
Emülatörde okuma ve kitap açma senaryoları CSS, iki yönlü ve XML (tablolu) kitaplarında bir önceki
sürümle aynı ekranları verdi. x4pro'da flash 384 bayt büyüdü, RAM değişmedi.

- [ ] İki ila dört sütunlu, çerçeveli ve çerçevesiz tablolar (sağdan sola bir tablo dahil) içeren bir
  kitabı dikey ve yatay ekranda açın: sütunlar, çerçeveler ve hücre boşlukları eskisi gibi görünmeli;
  büyük yazı boyutunda ya da tek hücreli satırlarda hücreler alt alta dizilmeli.

## Tek tek denetlenen etiket adları Rust'ta ("rust: our own chapter element names")

Bölüm ayrıştırıcısı açılan ve kapanan öğelerde `a`, `hr`, `span`, `sup`, `sub`, `ruby`, `rt`,
`html`, `body`, `li`, `ol`, `ul`, `p`, `pre`, `br`, `table`, `tr`, `td` ve `th` adlarını hâlâ tek
tek `strcmp` ile denetliyordu; artık sınıflandırma bu 19 ad için bir ad kimliği de veriyor ve C++ bu
kimliği karşılaştırıyor (adlar yine büyük/küçük harfe duyarlı; `body` açılışta büyük/küçük harf
gözetmeden aranmaya devam ediyor). Bir önceki commit (72df85eb) bu adlardan altısının yalnızca küçük
harfle işe yaradığını 1 testle sabitledi; bu commit de `span`'ların `id`'sinin çapa sayılmadığını
sınayan bir test ekledi (fark testi `id` üretmediği için bunu göremiyordu). Toplam 1044 host testi
geçiyor. Bölüm ayrıştırıcısının fark testi 5000 rastgele bölümde eski ve yeni sayfaları bayt bayt
aynı buldu; ad kimliklerine bilerek konan altı hatadan beşi yakalandı, `span`'ınkini yeni test
yakalıyor. Emülatörde okuma ve kitap açma senaryoları CSS, iki yönlü, heceleme ve XML kitaplarında,
JPEG senaryosu da resimli kitapta bir önceki sürümle aynı ekranları verdi. x4pro'da flash 96 bayt
küçüldü, RAM değişmedi.

- [ ] Listeler (numaralı ve madde işaretli), yatay çizgiler, satır sonları, önceden biçimlendirilmiş
  (`pre`) metin, üst ve alt simgeler, ruby metni, tablolar ve içindekilerden ya da dipnotlardan
  bağlantı verilen başlıklar içeren bir kitabı açın: hepsi eskisi gibi görünmeli, bağlantılar doğru
  yere gitmeli; KOReader eşitlemesinden sonra aynı paragrafa dönülmeli.

## Liste işaretleri ve ruby metni Rust'ta ("rust: our own list markers and ruby text cleanup")

Bölüm ayrıştırıcısı her liste öğesinin başına bir işaret koyar (`list-style-type: none` olan
listelerde koymaz): sırasız listelerde ve bir listenin dışındaki öğelerde madde işareti (•), sıralı
listelerde de listenin kendi sayacının sonraki değeri ve bir nokta (iç içe listeler kendi
sayaçlarını tutar). Ruby (furigana) metni de baştaki ve sondaki boşluk, sekme ve satır sonlarından
arındırılıp içindeki bu tür dizilerin her biri tek bir boşluğa indirilir. Bunlar artık
`bembeyaz-html`'de: işaret `list`, ruby metni `words` modülünde; açık listelerin yığını C++'ta
kaldı, her liste Rust'la ortak `bb_html_list` yapısında tutuluyor. Ruby metni artık yeni bir dizgeye
kopyalanmadan yerinde temizleniyor. Taşırken 10000 ve üstündeki numaralarda işaretin sonunu bozacak
bir hata yapıldı ve Rust birim testiyle yakalandı; işaretin sonuna NUL'u artık yalnızca C++ koyuyor.
Bir önceki commit (765dba1e) davranışı 2 testle sabitledi; toplam 1046 host testi geçiyor. Bölüm
ayrıştırıcısının fark testi 5000 rastgele bölümde eski ve yeni sayfaları bayt bayt aynı buldu; koda
bilerek konan beş hatadan dördü yakalandı, rastgele stillerde bulunmayan `list-style-type: none`'a
konanı sabitleme testi yakalıyor. Emülatörde okuma ve kitap açma senaryoları CSS, iki yönlü ve XML
kitaplarında bir önceki sürümle aynı ekranları verdi. x4pro'da flash 20 bayt büyüdü, RAM değişmedi.

- [ ] Numaralı (iç içe olanlar dahil), madde işaretli ve işaretsiz listeler ile ruby (furigana) metni
  içeren bir kitabı açın: numaralar, madde işaretleri ve ruby metni eskisi gibi görünmeli.

## Çizim temelleri Rust'ta ("rust: our own drawing primitives")

Arayüzün ve kitap sayfalarının çizgileri, dikdörtgen çerçeveleri ve dolu dikdörtgenleri (siyah,
beyaz, açık ve koyu gri titreme desenleri; seçim çubukları, düğmeler, ilerleme çubukları, tablo
çerçeveleri, gölgeli kutular, yuvarlatılmış dikdörtgenlerin düz kenarları) artık `bembeyaz-gfx`
crate'inde çiziliyor: mantıksal koordinatlar dört ekran yönelimiyle panelin bit düzenine eşleniyor,
kırpma dikdörtgeni ve gri ton şeritleri gözetiliyor. GfxRenderer her çizimde tuvali (tampon,
satırlar, panel boyu, yönelim, kırpma) `bb_gfx_canvas` olarak veriyor. Crate'in 5 birim testi (bir
mini-fuzz dahil) var. GfxRenderer host testlerinde derlenmediği için eski çizim kodu yeni
`test/gfx_primitives` host testine olduğu gibi kopyalandı; test her çalışmada 20000 rastgele tuvalde
iki yolu karşılaştırıyor, C++'ta kalan yuvarlatılmış köşe titremesinin de Rust'ın dolgu deseniyle
aynı olduğunu denetliyor (toplam 1048 host testi geçiyor). Geliştirme sırasında aynı düzenek,
rastgele panel boyları, yönelimler, şeritler ve kırpmalarla 12000 tuvalde 144000 çizimde eski ve
yeni tamponları bayt bayt, panel dışına düşen piksel sayılarını da aynı buldu; koda bilerek konan
altı hatadan beşi yakalandı (altıncısı şeridin son satırının ötesine yazmaktı, tampon tam o boyda
olduğundan zaten yazılamıyor). Çizgi ve çerçevelerde panel dışına düşen pikseller artık her biri
için ayrı ayrı koordinatlarıyla değil, çizim başına tek bir hata satırıyla günlüğe yazılıyor; tampon
yokken (bir derleme için ödünç verilmişken) çizilmeye çalışılırsa eskisi gibi çökmek yerine bir kez
uyarılıyor. Emülatörde ayarlar, uyku, kütüphane, BLE ayarları, alıntı, okuma ve kitap açma (CSS ve
XML), JPEG ve PNG senaryoları bir önceki sürümle aynı ekranları ve aynı çizim sürelerini verdi.
x4pro'da flash 316 bayt küçüldü, RAM değişmedi.

- [ ] Ana ekranı, menüleri, ayarları ve kütüphaneyi açın; bir kitabı dört Okuma Yönü'nde (Dikey,
  Yatay (Saat Yönü), Dikey 180°, Yatay (Saat Yönü Tersi)) açın: seçim çubukları, çerçeveler, ilerleme
  çubukları ve gri dolgular eskisi gibi görünmeli. Metin Yumuşatma (AA) açıkken tablolu, yatay
  çizgili ve resimli bir sayfa da eskisi gibi çizilmeli.

## Yuvarlatılmış dikdörtgenler Rust'ta ("rust: our own rounded rectangles")

Arayüzün yuvarlatılmış dikdörtgenleri artık `bembeyaz-gfx`'in `round` modülünde çiziliyor: çeyrek
yaylar, yuvarlatılmış çerçeveler (Lyra'nın düğme ipuçları, RoundedRaff'ın alt ipucu grupları, metin
seçimindeki düğmeler), yuvarlatılmış dolgular (açılır pencereler, menü satırları, ana ekranın seçili
kapak kutusu, metin seçimi tutamaçları; köşeleri tek tek seçilebilir, gri titreme desenleri dahil)
ve RoundedRaff'ın kapak resimlerinin köşelerini yuvarlatan maske. GfxRenderer eskiden bunları satır
satır ya da piksel piksel çiziyordu; şimdi her şekil için tek bir Rust çağrısı var ve C++'taki
`fillArc` ile `drawPixelDither` silindi. Crate'in 7 birim testi var; `test/gfx_primitives`'in eski
kod kopyasına bu dört şeklin eski C++ kodu da eklendi ve 20000 rastgele tuvalde iki yol bayt bayt
aynı, panel dışı piksel sayıları da aynı çıktı. Koda bilerek konan on hatadan dokuzunu yakaladı
(onuncusu eşdeğerdi: orta dolgunun fazladan sütunu zaten sağ sütunun ya da köşe yayının altında
kalıyor). Bir şey bilerek değişti: köşe maskesi de diğer çizimler gibi yazı tipi önbelleği taraması
sırasında hiçbir şey çizmiyor (yalnızca RoundedRaff'ın ana ekranı çağırıyor, orada tarama yok),
panel dışına düşen pikseller de her biri için ayrı satır yerine tek satırla günlüğe yazılıyor.
Emülatörde ayarlar, uyku, kütüphane, BLE ayarları, alıntı, okuma, kitap açma, JPEG ve PNG
senaryoları ile Lyra, Lyra Genişletilmiş, RoundedRaff, Cover Grid ve Klasik temalarının ekranları
(RoundedRaff'ın kapaklı ana ekranı dahil) bir önceki sürümle piksel piksel aynı çıktı. Flash
x4pro'da 156, x4c'de 328 bayt büyüdü, RAM değişmedi.

- [ ] Ayarlar → Ekran → Arayüz Teması'nda sırayla Klasik, Lyra, Lyra Genişletilmiş, RoundedRaff ve
  Cover Grid'i seçin; her birinde ana ekranı (kapaklı bir kitap açılmış olarak), menüleri, bir açılır
  pencereyi ve alttaki düğme ipuçlarını açın: yuvarlatılmış köşeler, seçili kutular ve RoundedRaff'ın
  kapaklarının yuvarlatılmış köşeleri eskisi gibi görünmeli; köşelerde kopukluk, fazladan nokta ya da
  kesik kenar olmamalı. (Cover Grid yalnızca PSRAM'li cihazda listelenir.)
- [ ] Dokunmatik ekranda bir kitapta metin seçin (Klasik ve Lyra temasında): seçim tutamaçları ve
  Sözlükte Ara / Clip / Yer İmi düğmeleri eskisi gibi görünmeli.

## Dolu çokgenler Rust'ta ("rust: our own filled polygons")

GfxRenderer'ın `fillPolygon`'u (Klasik temanın ana ekranında kapaksız kitabın yer imi şeridi,
FreeInkUI'nin üçgenleri: bugün yalnızca Kütüphane'nin sekme çubuğundaki sıralama oku) artık
`bembeyaz-gfx`'in `polygon` modülünde. Eski kod her çağrıda heap'te bir düğüm tamponu ayırıyor, her
satırda kesişimleri sıralıyor ve her pikseli ayrı ayrı çiziyordu; yeni kod tampon ayırmıyor (ayırma
hatası yolu da kalmadı), her satırın kesişimlerini kenarları yeniden gezerek sırayla buluyor ve
aralarını tek `fill_rect` ile dolduruyor. Bunun bedeli, satır başına işin nokta sayısının karesiyle
büyümesi; çağıranlar 3 ve 5 noktalı şekiller çiziyor. Kesişimler 64 bitle hesaplanıyor: eski kodun
büyük koordinatlardaki tanımsız taşması yok, değerler ancak i32 sınırlarına yakın koordinatlarda
sarar. Görünür tek fark: çokgen de diğer çizimler gibi yazı tipi önbelleği taraması sırasında hiçbir
şey çizmiyor. `test/gfx_primitives`'in eski kod kopyasına eski `fillPolygon` eklendi; rastgele 0–8
noktalı çokgenlerle 20000 tuvalde iki yol bayt bayt aynı, koda bilerek konan dokuz hatanın hepsi
yakalandı. Crate'in 8 birim testi var. Emülatörde Klasik temanın ana ekranındaki yer imi şeridi
(kart seçiliyken ve değilken), Kütüphane'nin iki yöndeki sıralama oku ve ayarlar, kütüphane, okuma,
kitap açma, uyku, alıntı ve BLE ayarları senaryoları bir önceki sürümle piksel piksel aynı çıktı.
Flash x4pro'da 64, x4c'de 240 bayt küçüldü, RAM değişmedi.

- [ ] Klasik temada kapaksız bir kitabı açıp ana ekrana dönün: kitap kutusundaki yer imi şeridi
  (alt ucu çentikli) eskisi gibi görünmeli. Kutu açılışta seçili olduğundan kart siyah, şerit beyaz
  çizilir; seçimi aşağı taşıyınca şerit siyah görünmeli.
- [ ] Kütüphane'yi açın: etkin sekmedeki üçgen sıralama oku eskisi gibi görünmeli. Sekmeler
  seçiliyken Onay'a uzun basın (dokunmatik ekranda etkin sekmeye dokunun): sıralama yönü ve ok
  dönmeli.

## Resimler Rust'ta ("rust: our own picture drawing")

GfxRenderer'ın `drawBitmap` ve `drawBitmap1Bit`'i (ana ekranlardaki kapak küçük resimleri, uyku
ekranının kapak ya da özel resmi ve kaplaması, resim görüntüleyici), `drawIcon` (kapaksız kitapların
simgeleri, Lyra'nın ana menü simgeleri, Cover Grid'in sekme ve boş ana ekran simgeleri) ve gece
modunda resimlerin kendi renklerini koruyan `preserveImagePolarity` (bu resimler ve okuyucudaki
kitap resimleri) artık `bembeyaz-gfx`'in `image` modülünde: kırpma, kutuya sığacak kadar küçültme,
üstten ya da alttan başlayan satırlar, siyah-beyaz ve iki gri düzlem (fark ya da mutlak seviyeler),
beyazın saydam sayılması ve 1 bitlik dosyaların ayrı kuralları. Dosyadan satır okuyan `Bitmap` ve
satır tamponu C++'ta; Rust her satırı geri çağrıyla istiyor ve tampon bir satırdan kısaysa hiç
çizmiyor. Eski iki ayrı yol tek döngüde birleşti; farkları (1 bitlik yolun kırpmaması, her düzlemde
siyah çizmesi, ekranın altındaki satırları okuyup geçmesi) bir bayrakla korunuyor. Eski kodun bir
tuhaflığı bilerek korundu: alttan başlayan bir resmin ilk satırı ekranın altına düşüyorsa resim hiç
çizilmiyor. `drawIcon` ve `drawBitmap1Bit` artık diğer çizimler gibi yazı tipi önbelleği taraması
sırasında hiçbir şey çizmiyor. `test/gfx_primitives`'in eski kod kopyasına eski resim kodu
(bellekteki sahte bir `Bitmap` ile) eklendi; 6000 rastgele resim, simge ve ters çevirmede iki yol
bayt bayt aynı, okunamayan satır da aynı. Koda bilerek konan hatalardan eşdeğer olmayanların hepsi
yakalandı. Crate'in 11 birim testi var. Emülatörde resim görüntüleyicide yedi BMP (1, 2, 4, 8, 24,
32 bit ve kesik dosya), uyku ekranı, JPEG ve PNG resimli kitaplar, gece modu açıkken kapaklı Lyra ve
RoundedRaff ana ekranları ile kitap açma, Lyra Genişletilmiş ve RoundedRaff ana ekranları, kütüphane
ve okuma senaryoları bir önceki sürümle piksel piksel aynı çıktı; resim görüntüleyicide resim başına
çizim aynı ya da 30–70 ms kısa (24 ve 32 bitlik BMP'ler). Flash x4pro'da 688, x4c'de 936 bayt büyüdü
(piksel yazımı artık satır içinde; eski kod her piksel için `drawPixel` çağırıyordu), RAM değişmedi.

- [ ] Gece modu kapalı ve açıkken kapaklı (JPEG ve PNG) kitaplarla Klasik, Lyra, Lyra Genişletilmiş,
  RoundedRaff ve Cover Grid ana ekranlarını açın (Cover Grid yalnızca PSRAM'li cihazda): kapak küçük
  resimleri eskisi gibi, gece modunda da kendi renkleriyle görünmeli; kapaksız kitapta kitap simgesi,
  Lyra'da ana menü simgeleri, Cover Grid'de sekme simgeleri eskisi gibi olmalı.
- [ ] Gece modu kapalı ve açıkken JPEG ve PNG resimli bir kitabı açın, bir resmin olduğu sayfaya
  gidip geri dönün (ilk açılış ve önbellekten açılış): resimler kendi renkleriyle görünmeli.
- [ ] Uyku Ekranı'nı Özel yapıp kartın köküne `/sleep.bmp` (ya da `/.sleep` klasörüne BMP'ler) koyun;
  Uyku Ekranı Kapak Modu'nu Sığdır ve Kırp, Uyku Ekranı Kapak Filtresi'ni Yok ve Negatif ile deneyin:
  resmin kırpılması, ölçeği ve gri tonları eskisi gibi olmalı. 1 bitlik bir resmi Kırp ile de deneyin.
  Ayrıca Uyku Ekranı'nı Kapak yapıp bir kitaptan uykuya geçin.
- [ ] Uyku Ekranı'nı Transparent yapıp kartın köküne saydamlık kanalı olmayan (24 bit)
  `/sleep-overlay.bmp` koyun ve okurken uykuya geçin: resim sayfanın üstünde, beyazları saydam olarak
  görünmeli.
- [ ] Dosyalara Göz At'tan 1, 2, 4, 8, 24 ve 32 bitlik BMP'leri resim görüntüleyicide, gece modu
  kapalı ve açıkken açın: eskisi gibi görünmeli.

## Glifler Rust'ta ("rust: our own glyph drawing")

Ölçeklenmemiş her glif (kitap metninin gövdesi, menüler, düğme ipuçları, ekran klavyesinin yan
düğmelerindeki döndürülmüş yazılar) artık `bembeyaz-gfx`'in `glyph` modülünde çiziliyor: mantıksal
kırpma, metnin döndürülmesi ve panel yönelimi tek bir dik dönüşümde birleşiyor, şerit (gri ton
bandı) kırpması ve adresleme glif başına bir kez hesaplanıyor; 1 ve 2 bitlik pikseller, siyah-beyaz
ve iki gri düzlem kuralları eskisiyle aynı. Bu yoldan geçmeyenler eskisi gibi: üst ve alt simgeler
ile küçük büyük harfler (ölçeklenmiş glifler) ve büyük ilk harf (drop cap) piksel piksel, fontta
olmayan karakterlerin kutuları dolu dikdörtgen olarak çiziliyor. `GlyphBitmap.h`'te yalnızca
yerleşim yapısı kaldı; dik olmayan bir yerleşim artık hiçbir şey çizmiyor (çağıranlar hep dik
yerleşim veriyor). İlk sürüm sayfa çizimini %20 yavaşlattı; dikey yönelimde glif satırının panelde
tek bir bit sütununa düştüğü yol ayrıldı (maske satır başına bir kez, bayt adımı sabit), 2 bitlik
tam baytlar sabit kaydırmalarla açıldı, düzlem kuralları derleme zamanına, siyah/beyaz seçimi dalsız
maskeye taşındı, panel ekseni tek bir yönelim eşleşmesiyle bulunuyor. Emülatörde sayfa çizim
süreleri eskisiyle neredeyse aynı: dikeyde üç sayfada siyah-beyaz 44–45 ms (eski 44), gri düzlemler
216 ve 249 ms (eski 215 ve 246, en çok %1); yatayda bir sayfada 11 (eski 10), 69–70 ve 80 ms (eski
69 ve 79). Bunun bedeli flash: x4pro'da 3324, x4c'de 3440 bayt (dört düzlem ve derinlik için ayrı
döngüler), RAM değişmedi; yığın çerçevesi 256 bayt (eski iç içe 272). Mevcut `test/glyph_bitmap`
testinin piksel piksel referansı artık Rust'a karşı çalışıyor; eski `GlyphBitmap.h` test dizinine
olduğu gibi (yalnızca ad alanı değişerek) kopyalandı ve 20000 rastgele glifte (sekiz dik glif yönü,
dört panel yönelimi, şeritler, kırpmalar, kimi 8'in katı olmayan panel genişlikleri) iki yol bayt
bayt aynı. Son koda bilerek konan 24 hatanın hepsi yakalandı. Crate'in 12 birim testi var.
Emülatörde ayarlar, kütüphane, kitap açma (CSS, iki yönlü ve XML kitapları dahil), okuma (dikey ve
yatay), uyku, alıntı, BLE ayarları, BMP görüntüleyici, gece modu ve Klasik, Lyra Genişletilmiş,
RoundedRaff ve Cover Grid ana ekranları bir önceki sürümle piksel piksel aynı çıktı.

- [x] Bir kitabı dört Okuma Yönü'nde (Dikey, Yatay (Saat Yönü), Dikey 180°, Yatay (Saat Yönü Tersi)),
  Metin Yumuşatma (AA) kapalı ve açık okuyun: harfler AA kapalıyken keskin, açıkken gri kenarlı ve
  eskisi gibi görünmeli; sayfa kenarlarında ve kırpılan yerlerde kesik ya da kaymış harf olmamalı.
- [ ] Seri günlükte `Page render:` satırlarındaki `bw_render`, `gray_lsb` ve `gray_msb` sürelerini
  dikey ve yatay okumada bir önceki sürümle (64b27c5b) karşılaştırın: aynı ya da çok yakın olmalı
  (emülatörde dikeyde sayfa başına yaklaşık 15/72/83 ms); belirgin bir yavaşlama varsa bildirin.
- [ ] Dokunmatik olmayan bir cihazda ekran klavyesini açın (ör. bir Wi-Fi şifresi girerken): yan
  düğmelerdeki döndürülmüş ">" ve "<" yazıları eskisi gibi görünmeli. Menüleri ve alttaki düğme
  ipuçlarını da gözden geçirin.

## Ölçeklenmiş glifler Rust'ta ("rust: our own scaled glyphs")

Üst ve alt simgeler (yarı boy) ile küçük büyük harfler (dörtte üç boy) ve büyük ilk harfler (drop
cap) artık `bembeyaz-gfx`'in `scaled` modülünde çiziliyor. Küçültmede her piksel kapladığı kaynak
bloğuna bakıyor: yarıya inerken bloktaki herhangi bir mürekkep (2 bitte koyu gri ya da iki açık gri)
çiziliyor, ince çizgiler kaybolmuyor; dörtte üçte çoğunluk aranıyor, gövdeler kalınlaşmıyor. Büyük
ilk harfte her pikselin merkezi glife geri eşleniyor ve dört komşunun çift doğrusal kaplaması
siyahın yarısına ulaşınca çiziliyor. Glifin bulunması, ilerleme genişliği ve konum C++'ta; eskiden
her piksel için ayrı bir `drawPixel` çağrısı vardı, şimdi glif başına bir Rust çağrısı ve panel
dışına düşen pikseller için tek bir günlük satırı. Rust 4096 pikselden büyük glifleri ve 16 kattan
büyük ölçekleri çizmiyor (bölümler en çok 16 kat saklar; bozuk bir bölüm önbelleği eskiden 65 bin
kata kadar bir döngü başlatabilirdi). `test/gfx_primitives`'in eski kod kopyasına iki eski döngü
eklendi; 6000 rastgele glifte (1 ve 2 bit, yarı ve dörtte üç boy, 1/256 ile 16 kat arası ölçek —
bölümler 1 ile 16 kat arasını saklar —, kimi 255 piksellik glifler, dört yönelim, şeritler,
kırpmalar) iki yol bayt bayt ve panel dışı sayısıyla aynı; koda bilerek konan 12 hatanın hepsi
yakalandı. Emülatörde CSS kitabının küçük büyük harfli başlığı, üç satırlık büyük ilk harfi ve üst
simgeli sayfası bir önceki sürümle piksel piksel aynı, çizim süreleri de aynı (siyah-beyaz 10, gri
düzlemler 69 ve 79 ms). x4pro'da flash 540, x4c'de 424 bayt büyüdü, RAM değişmedi.

- [ ] Üst simgeli dipnot numaraları, alt simgeler, küçük büyük harfli başlıklar ve büyük ilk harfle
  başlayan paragraflar içeren bir kitabı dört Okuma Yönü'nde, Metin Yumuşatma (AA) kapalı ve açık
  açın: bunlar eskisi gibi görünmeli.

## Ekran bölgesi kopyaları Rust'ta ("rust: our own screen region copies")

Ekranın bir parçasını yeniden çizmeden saklayıp geri koyan beş GfxRenderer işlevi
(`readFramebufferRegion`/`writeFramebufferRegion`: okuyucuda dipnot seçerken vurgunun altı;
`getRegionByteSize`/`copyRegionToBuffer`/`copyBufferToRegion`: Lyra, Klasik ve RoundedRaff ana
ekranlarının kapak tamponu, Cover Grid'in kapak önbelleği ve Uyku Ekranı "Transparent" iken uykuya
geçerken çıkan açılır pencerenin arka planı) artık `bembeyaz-gfx`'in `region` modülünde: mantıksal
dikdörtgenin panelde kapladığı baytlar (8 piksele hizalı ya da dokunulan baytlar) bulunuyor, satırlar
tampona kopyalanıp geri yazılıyor. Bilerek değişen tek şey: çerçeve tamponu yokken (bir derlemeye
ödünçken) `readFramebufferRegion`/`writeFramebufferRegion` eskisi gibi çökmek yerine hiçbir şey
yapmıyor. `test/gfx_primitives`'in eski kod kopyasına eski beş işlev eklendi; 20000 rastgele
dikdörtgende (8'in katı olmayan panel genişlikleri, dört yönelim, kısa tamponlar) boyutlar, kopyalar ve
geri yazılan ekran bayt bayt aynı. Emülatörde kapaklı Lyra, RoundedRaff ve Cover Grid ana ekranları,
uyku, kütüphane, okuma ve ayarlar bir önceki sürümle piksel piksel aynı. Flash x4pro'da 340, x4c'de 276
bayt büyüdü, RAM değişmedi.

- [ ] Kapaklı bir kitapla Lyra, Klasik ve RoundedRaff ana ekranlarında seçimi kapak ile menü arasında
  birkaç kez gezdirin: kapak her seferinde eksiksiz geri gelmeli. Cover Grid temasında (PSRAM'li
  cihazda) kapaklar arasında ve sekmeler arasında gezinin: önbellekten gelen kapaklar eksiksiz olmalı.
- [ ] En az iki dipnotu olan bir sayfada okuyucu menüsünden "Bağlantılar ve dipnotlar"ı açıp
  dipnotlar arasında ok ya da yan düğmelerle gezinin: vurgu taşınınca eski yerinde iz kalmamalı.
- [ ] Uyku Ekranı'nı Transparent yapıp kartın köküne bir kaplama resmi (`/sleep-overlay.bmp` ya da
  `.png`) koyun; okurken (bir de döndürülmüş bir Okuma Yönü'nde) ve ana ekrandayken uykuya geçin:
  kaplamanın altında "Uyku moduna geçiliyor" penceresinden iz kalmamalı.

## Alttan başlayan resimlerin kırpılması düzeltildi ("fix: draw bottom-up pictures that start below the screen")

Alttan başlayan bir resim (BMP'lerin çoğu böyle saklanır) ekranın altına taşıyorsa, yani dosyadaki ilk
satırı ekranın altına düşüyorsa, eskiden hiç çizilmiyordu: gri ton yolu ilk satırda "ekranın altı"
deyip duruyordu. Üstten başlayan aynı resim görünen kısmıyla çiziliyordu. Artık alttan başlayan resim
o satırları okuyup geçiyor ve ekrana geri gelen satırları çiziyor. Bugünkü ekranlar resmi hep ekrana
sığdırdığı için görünür bir fark beklenmiyor; emülatörde resim görüntüleyici, uyku ve kapaklı ana
ekran bir önceki sürümle aynı. Crate testi ve eski kodla karşılaştırma bu durumu sabitliyor.

- [ ] Ekrandan büyük, kırpmasız gösterilen bir BMP varsa (ör. Uyku Ekranı'nda Kırp ile büyük bir
  resim) eskisi gibi ya da daha doğru (alt kısmı kesik değil, görünen kısmı çizilmiş) görünmeli.

## Titreme Rust'ta ("rust: our own dithering")

Resimlerin panelin dört gri seviyesine ya da siyah-beyaza çevrilmesi (BMP resim görüntüleyici ve uyku
ekranının özel resimleri, JPEG ve PNG kapakların ve ana ekran küçük resimlerinin BMP'ye dönüştürülmesi)
artık yeni `bembeyaz-dither` crate'inde: Atkinson ve Floyd-Steinberg hata yayma, 1 bitlik Atkinson,
panele ayarlı eşikler, doğal palet ve (derleme zamanında kapalı) parlaklık ayarı. Eskiden de hiçbir
yoldan ulaşılmayan gürültülü 1 bitlik eşik kaldırıldı. `Bitmap`, PNG ve JPEG kapak dönüştürücüleri
artık her piksel için değil, satır ya da 64 piksellik parça başına bir çağrı yapıyor; hata
satırlarının belleği C++'taki tek `Ditherer` sınıfında (eski üç sınıfın yerine; `Bitmap` onu yerinde
tutuyor, yeni bir bellek ayırma ya da hata yolu yok). Rust döngüsü hataları dizinlemeden, yazmaçta
kaydırarak taşıyor: sınır denetimi ve panik yolu yok. İlk sürüm kapak küçük resmini %10
yavaşlatmıştı; şimdi eskisiyle aynı ya da biraz hızlı (emülatörde JPEG küçük resmi 230–233 ms, eski
232–235; resim görüntüleyicide yedi BMP toplamda 128 ms hızlı). Eski titreme sınıfları ve işlevleri
olduğu gibi `test/dither`'a kopyalandı: 4000 rastgele resimde (her yöntem, rastgele parçalar, satır
sınırları) çıktı bayt bayt aynı ve son baytın resim dışındaki bitleri değişmiyor. Koda bilerek konan
21 hatanın hepsi yakalandı. PNG→BMP'nin 841 durumluk özet testi ve BMP okuyucu testleri de değişmeden
geçiyor. Emülatörde resim görüntüleyici, uyku, JPEG ve PNG kapaklı ana ekranlar bir önceki sürümle
piksel piksel aynı. Flash x4pro'da 780, x4c'de 848 bayt büyüdü (yöntem başına ayrı döngüler), statik
RAM değişmedi.

- [ ] Dosyalara Göz At'tan 24 ve 32 bitlik BMP'leri açın: gri tonlar ve titreme deseni eskisi gibi
  olmalı.
- [ ] Uyku Ekranı'nı Özel yapıp `/sleep.bmp` (ya da `/sleep` klasörüne) renkli bir BMP koyun; Uyku
  Ekranı Kapak Filtresi'ni Yok ve Kontrast ile, Uyku Ekranı Kapak Modu'nu Sığdır ve Kırp ile deneyin;
  Uyku Ekranı'nı Kapak yapıp bir kitaptan uykuya da geçin: resimler eskisi gibi görünmeli.
- [ ] Daha önce açılmamış JPEG ve PNG kapaklı kitapları açıp Ana Sayfa'ya dönün (küçük resim orada,
  "Yükleniyor" sırasında oluşur): kapaklar eskisi gibi görünmeli, bekleme belirgin biçimde uzamamalı.

## Önbellekteki resim satırları Rust'ta ("rust: our own cached image rows")

Bölüm resimleri ilk kez çözüldükten sonra (ilk gösterimin gri geçişleri dahil) önbellekten (2 bitlik
gri seviye satırları) çiziliyor; bu satırların ekrana yazılması artık `bembeyaz-gfx`'in `levels` modülünde (`GfxRenderer::drawLevelRow`).
Yönelim satır başına bir kez çözülüyor, yalnızca panelde ve o anki gri ton şeridinde kalan sütunlar
geziliyor ve her sütun bir öncekinden tek bir bit adımı ötede: eski `DirectPixelWriter` döngüsü her
piksel için seviyeyi açıp konumu yeniden hesaplıyordu. Önbelleğin okunması ve resmin ilk kez çözülüp
önbelleğe yazılması değişmedi (JPEG ve PNG dönüştürücüleri hâlâ `DirectPixelWriter` kullanıyor).
Eski yazıcı olduğu gibi `test/gfx_primitives`'e kopyalandı: 20000 rastgele satırda (dört yönelim,
siyah-beyaz ve iki gri düzlem, mutlak ve fark gri düzlemleri, şeritler; eski yazıcı ekranın dışına
taşan satırları desteklemediği için yalnızca ekrandakiler) tampon bayt bayt aynı. Ekranın dışına
taşan satırların kesilmesini crate testleri sınıyor (önbellekteki resim bir sütun ya da satır geniş
olabildiği için ekranın kenarına dayanan resimde eski yazıcı bu pikseli bir sonraki panel satırına,
şeridin son satırında tamponun bir bayt ötesine yazıyordu; şimdi atılıyor). Koda bilerek konan 10 hatanın hepsi yakalandı
(ikisi yalnızca bu kenar testleriyle). Emülatörde JPEG ve PNG resimli bölümlerde gri düzlem
çizimi %11–15 kısaldı (sayfa başına ortanca 97–98→85–86 ve 86→73–74 ms), ekran görüntülerinin hepsi bir önceki
sürümle aynı. Flash x4pro'da 460, x4c'de 480 bayt büyüdü, statik RAM değişmedi.

- [ ] JPEG ve PNG resimli bir EPUB'ı açın, resimli sayfaları ileri geri çevirin: resimlerin gri tonları
  ve kenarları bir önceki sürümdeki gibi görünmeli (gri tonlar ilk gösterimde de önbellekten çizilir).
- [x] Aynı sayfaları dört yönelimde (Okuma Yönü ayarı) açın: resim yerinde olmalı, kayma ya da yarım
  kalmış şerit olmamalı.

## JPEG resimlerinin çizimi Rust'ta ("rust: our own JPEG image sampling")

Bölüm resimleri, OPDS kataloğundaki kitapların Details (Ayrıntılar) ekranındaki kapak, resim
görüntüleyicide açılan ve orada "Kapak Ayarla" ile uyku resmi yapılan JPEG'ler çözülürken ekrana
böyle çiziliyor: çözücünün verdiği gri bloklar çıkış boyutuna getiriliyor (iki eksende de
büyütmede çift doğrusal, 1:1'de kopya, diğer durumlarda en yakın piksel), 4×4 Bayer deseniyle ya da
titremesiz üçte bir eşikleriyle panelin dört seviyesine çevriliyor ve satır satır ekrana ve
önbellek bandına yazılıyor. Bunların hepsi artık `bembeyaz-dither`'ın `sample` modülünde; satırlar
ekrana önceki işteki seviye satırlarıyla yazılıyor. Eskiden her piksel için bir `DirectPixelWriter`
ve bir önbellek yazımı vardı; şimdi bloğun her satırı için, en çok 512 sütunluk parçalar halinde üç
çağrı var (`bb_dither_block_row`, `drawLevelRow`, `bb_dither_put_levels`). Eski çift doğrusal
döngüde bir hata düzeltildi: ters ölçek aşağı yuvarlandığı için bir bloğun ilk sütunu kaynakta
-1'e düşebiliyor ve iç döngüde (bir sütunluk bloklarda sağ kenar döngüsünde de) satırdan önceki
bayt okunuyordu (ilk satırda tamponun bir bayt öncesi). Bu, blok sınırlarında tek tek piksellerin
bir seviye kaymasına yol açabiliyordu; şimdi her sütun bloğun içinde kalıyor. Eski geri çağrı
`test/image_sampling`'e kopyalandı (bu düzeltmeler işaretli): 20000 rastgele resimde (ölçekler,
kırpma, konum, ekrandan taşma, dört yönelim, üç düzlem, şeritler, blok boyutları, titreme açık ya
da kapalı, önbellekli ya da önbelleksiz) ekran ve önbellek bayt bayt aynı. Koda bilerek konan 18
hatanın 16'sını bu test, birini crate testleri yakaladı; kalanı sonucu değiştirmiyor (255 ile 254
aynı seviyeye düşüyor). Emülatörde JPEG resimli bölüm ve bozuk JPEG senaryoları bir önceki sürümle
piksel piksel aynı, sayfa süreleri değişmedi (çözme süresi baskın). Flash x4pro'da 440, x4c'de 448
bayt büyüdü, statik RAM değişmedi; yığında geri çağrı başına 128 baytlık bir tampon.

- [ ] JPEG resimli bir EPUB'ı ilk kez açın (önbellek yokken resim çözülürken çizilir): resimler
  eskisi gibi görünmeli; büyütülen küçük resimlerde blok sınırlarında çizgi ya da kayma olmamalı.
- [ ] OPDS kataloğunda JPEG kapaklı bir kitabın Details (Ayrıntılar) ekranını açın: kapak eskisi
  gibi görünmeli.
- [ ] Dosyalara Göz At'tan bir JPEG açın: eskisi gibi görünmeli. Uyku Ekranı Kapak Modu'nu önce Kırp
  yapın, ekrandan büyük bir JPEG'de Kapak Ayarla deyin ve uykuya geçin; sonra Sığdır ile tekrarlayın:
  uyku resmi eskisi gibi, kırpması doğru olmalı.

## PNG resimlerinin çizimi Rust'ta ("rust: our own PNG image sampling")

PNG'ler (bölüm resimleri, OPDS kataloğundaki kitapların Details (Ayrıntılar) ekranındaki kapak,
resim görüntüleyici ve saydam uyku kaplaması `/sleep-overlay.png`) çözülürken her çıkış satırı
artık JPEG'lerdeki gibi en çok 512 sütunluk parçalar halinde üç çağrıyla çiziliyor: `bembeyaz-dither`
sütunları kaynak satırda Bresenham adımıyla örnekliyor (sütun c, kaynakta kırpma + c × görünen
genişlik / çıkış genişliği pikselini okur; parçanın başı kapalı biçimde hesaplanıyor), Bayer
deseniyle ya da titremesiz üst iki bitle seviyelere çeviriyor ve saydam resimlerde alfayı 4×4
düzenli eşikle bir sütun maskesine çeviriyor. Satırlar ekrana seviye satırlarıyla yazılıyor; bu yazıcı
artık bir maske alıyor ve maskeli satırlarda (yalnızca saydam kaplama) siyah-beyazda beyazı da
yazıyor (eski yazıcının `writeWhiteInBw`'si). Böylece `DirectPixelWriter.h` ve `DitherUtils.h`
kullanılmaz oldu ve silindi; eski kopyaları testlerde duruyor. Kaynak satırın griye çevrilmesi ve
satır seçimi değişmedi. Eski geri çağrı olduğu gibi `test/image_sampling`'e kopyalandı: 20000
rastgele resimde (kırpma, sığdırma ve tam boyut (büyütme dahil), konum, ekrandan taşma, dört yönelim,
üç düzlem, şeritler, titreme, saydamlık, önbellek; testte parçalar 24 sütun) ekran ve önbellek bayt
bayt aynı. Koda bilerek konan 13 hatanın 11'ini bu test, birini crate testi yakaladı (parçanın 4'ün
katında başlamadığı eşik deseni; dönüştürücüde parçalar hep 4'ün katında başlıyor); kalanı sonucu
değiştirmiyor. Emülatörde PNG ve JPEG resimli bölümler ile bozuk PNG bir önceki sürümle piksel piksel
aynı, süreler değişmedi. Saydam PNG kaplaması da emülatörde (`emu/sleep.txt`, `make_bmp_files.py`'nin `overlay-png`
kartı) bu değişiklikten önceki sürümle (38e2e5ef) piksel piksel aynı çizildi. Flash x4pro'da 1656, x4c'de 1624 bayt büyüdü (maskeli ve maskesiz iki
yazma döngüsü, örnekleme), statik RAM değişmedi; yığında geri çağrı başına 192 baytlık tampon (JPEG'in tamponu gibi artık sıfırla
başlatılıyor).

- [ ] PNG resimli bir EPUB'ı ilk kez açın ve sayfaları çevirin: resimler eskisi gibi görünmeli.
- [ ] Dosyalara Göz At'tan bir PNG açın: eskisi gibi görünmeli.
- [ ] OPDS kataloğunda PNG kapaklı bir kitabın Details (Ayrıntılar) ekranını açın: kapak eskisi gibi
  görünmeli.
- [x] Uyku Ekranı'nı Transparent (Türkçe çevirisi yok) yapıp kartın köküne saydam bir
  `sleep-overlay.png` koyun (kenarı opak, içi giderek saydamlaşan bir halka gibi; kökte
  `sleep-overlay.bmp` olmasın, önce o seçilir) ve uykuya geçin: kaplama ekrandakinin üstüne eskisi
  gibi binmeli; saydam yerlerde alttaki görünmeli, opak beyaz yerler beyaz olmalı.

## PNG satırlarının griye çevrilmesi Rust'ta ("rust: our own PNG row gray")

PNG'ler çözülürken her kaynak satırın griye çevrilmesi (PngToFramebufferConverter'ın
`convertLineToGray`'i: bölüm resimleri, OPDS kataloğundaki Details (Ayrıntılar) kapağı, resim
görüntüleyici, saydam uyku kaplaması) artık `bembeyaz-png`'nin `gray` modülünde: piksel türüne göre
luma (256'da 77/150/29), 1, 2 ve 4 bitlik örneklerin 255'e ölçeklenmesi, palet ve paletin alfası,
tRNS renk anahtarının saydam sayılması, alfanın kaplama için ayrı tutulması ya da beyaza
karıştırılması, bilinmeyen türlerde orta gri. Örnekler bölmesiz okunuyor; dizinler sınır denetimli,
satırın dışı 0 okunuyor (eski kod satırın yeterince uzun olduğunu varsayıyordu). Paletsiz satır (çözücü hep palet
verdiği için olmayan bir durum) artık orta gri; eskiden dizinli olmayan satırlar paletsiz de
çevriliyor, dizinli satırlar gri ölçeği gibi okunuyordu. Eski işlev olduğu gibi `test/image_sampling`'e kopyalandı: 20000 rastgele satırda (beş
piksel türü ve iki bilinmeyen tür, 1, 2, 4 ve 8 bit, renk anahtarı eşleşen ya da eşleşmeyen, alfalı
ya da alfasız, kaplama ya da değil) gri ve alfa bayt bayt aynı. Koda bilerek konan 15 hatanın 14'ü
yakalandı; kalanı sonucu değiştirmiyor (1, 2 ve 4 bitte en büyük değer 255'i böler). Emülatörde PNG
kitabı (her renk türü ve bit derinliği, palet ve renk anahtarı saydamlığı, alfa) ve bozuk PNG bir
önceki sürümle piksel piksel aynı, süreler değişmedi; saydam PNG uyku kaplaması da (`emu/sleep.txt`)
PNG değişikliklerinden önceki sürümle (38e2e5ef) aynı. Flash x4pro'da 1332, x4c'de 1220 bayt büyüdü
(her piksel türü için alfalı ve alfasız ayrı döngüler), statik RAM değişmedi.

- [ ] PNG resimli bir EPUB'ı (renkli, gri, paletli, saydam arka planlı resimlerle) ilk kez açın:
  resimler eskisi gibi görünmeli; saydam arka planlar beyaz olmalı.
- [x] Saydam uyku kaplaması (`/sleep-overlay.png`, önceki maddedeki gibi): kaplama eskisi gibi
  binmeli.

## Kapak satırlarının ölçeklenmesi Rust'ta ("rust: our own cover scaling")

Kapakların BMP'ye çevrilmesinde (ana ekran küçük resimleri, Uyku Ekranı'nın Kapak modu) satırların
çıkış genişliğine getirilmesi artık `bembeyaz-dither`'ın `scale` modülünde: JpegToBmpConverter ve
PngToBmpConverter'daki aynı alan ortalaması döngüsü (her çıkış sütununun kapladığı kaynak
sütunlarının toplanması) tek bir işlevde, ortalamaların 64 piksellik parçalarla titretilmesi
`scale` modülünün `area_pixels`'inde (C++'ta `Ditherer::areaPixels` ile çağrılıyor) ve JPEG'in ilerlemeli resimleri büyütürken kullandığı doğrusal satır
örneklemesi ile iki satırın karıştırılması da orada. Hangi kaynak satırın hangi çıkış satırına
düştüğü (satır durumu), BMP yazımı ve 8 bitlik çıkış yolu (derleme zamanında kapalı) C++'ta kaldı.
Okumalar sınır denetimli. Eski döngüler olduğu gibi `test/dither`'a kopyalandı: 4000 rastgele
durumda (genişlikler, dönüştürücülerin ölçeği ya da taşan rastgele ölçekler, birkaç satırlık
toplamlar, beş titreme yöntemiyle iki satır, doğrusal ve karıştırılmış satırlar) sonuçlar bayt bayt aynı;
PNG kapak dönüştürücüsünün 841 durumluk özet testi de değişmeden geçiyor. Koda bilerek konan 13
hatanın hepsi yakalandı. Emülatörde JPEG ve PNG kapaklı ana ekranlar bir önceki sürümle piksel
piksel aynı; JPEG küçük resmi 233 yerine 227 ms, PNG küçük resmi 397 yerine 388 ms sürdü. Flash
x4pro'da 284, x4c'de 316 bayt büyüdü, statik RAM değişmedi.

- [ ] Daha önce açılmamış JPEG ve PNG kapaklı kitapları açıp Ana Sayfa'ya dönün: küçük resimler
  eskisi gibi görünmeli.
- [x] Uyku Ekranı'nı Kapak yapıp bir kitaptan uykuya geçin; Uyku Ekranı Kapak Modu'nu Sığdır ve Kırp
  ile deneyin: kapak eskisi gibi görünmeli.
- [ ] İlerlemeli (progressive) JPEG kapaklı bir kitap varsa onun küçük resmi de eskisi gibi
  yumuşak görünmeli.

## Resim boyutu yoklaması Rust'ta ("rust: our own image size probe")

Bölüm derlenirken her resmin boyutu, resmin tamamı açılmadan EPUB'dan akan ilk baytlarından
bulunuyor (`ImageDimsProbe`): JPEG'de işaretçi bölümleri (her boyuttaki EXIF/APPn, tamponlamadan)
bir çerçeve başlığına (SOFn) dek atlanıyor, PNG'de IHDR'deki genişlik ve yükseklik sabit
konumlarından okunuyor. Bu durum makinesi artık yeni `bembeyaz-image` crate'inde; durumu C++
sınıfının içinde 28 baytlık opak `bb_image_probe` belleğinde duruyor ve `bb_image_probe_init` ile
kuruluyor (yeni bir bellek ayırma yok), boyutun sınır denetimi ve günlük satırı C++'ta. Crate'in
kısaltılmış ve bozulmuş başlıklarla mini-fuzz testi var. Eski ayrıştırıcının doğrudan bir testi yoktu; olduğu gibi
`test/image_probe`'a kopyalandı: 20000 rastgele başlıkta (her türden JPEG işaretçisi ve bölüm
uzunluğu, dolgu baytları, çerçeve başlığı ya da öncesinde tarama veya dosya sonu, PNG imzası ve
IHDR, rastgele bozulmalar ve çöp, rastgele parçalar halinde) alınan bayt sayıları ve bulunan boyut
aynı. Koda bilerek konan 17 hatanın hepsi yakalandı (biri crate testiyle). Emülatörde JPEG ve PNG
resimli kitaplar ile bozuk JPEG bir önceki sürümle piksel piksel aynı, resim çıkarma günlük
satırları da aynı. Flash x4pro'da 100, x4c'de 188 bayt büyüdü, statik RAM değişmedi.

- [x] JPEG ve PNG resimli bir EPUB'ı ilk kez açın: resimler eskisi gibi yerleşmeli (boyutları
  doğru, sayfa sonları eskisi gibi) ve ilk açılış eskisi kadar hızlı olmalı (yoklama başarısız
  olursa resim bütünüyle çıkarılır: doğru ama resim başına saniyeler sürer).

## PNG kapak okuyucusunda bölmesiz örnekler ("png: cover samples without division")

`bembeyaz-png`'nin kapak okuyucusu 1, 2 ve 4 bitlik gri ve paletli satırlardaki örnekleri artık
bölmeden (bit kaydırarak) okuyor ve griyi 255'i bölen bir çarpanla ölçekliyor; satır filtresi de
piksel adımına sıfır gelmeyeceği belli olarak hesaplanıyor. Sonuçlar aynı, yalnızca sıfıra bölme
panik yolları ve bölmeler kalktı: flash x4pro'da 240, x4c'de 244 bayt küçüldü. PNG kapak
dönüştürücüsünün 841 durumluk özet testi (her renk türü ve bit derinliği) ve crate testleri
değişmeden geçiyor; emülatörde PNG kitabı piksel piksel aynı.

- [ ] 1, 2 ya da 4 bitlik gri veya paletli PNG kapaklı bir kitabın küçük resmi eskisi gibi
  görünmeli.

## Sözlük dizini ve aramaları Rust'ta ("rust: our own dictionary index")

StarDict sözlüklerinde (`/dictionaries/<klasör>/`) `.ifo`'dan 64 bitlik konum ve HTML tanım
bilgisinin okunması, ilk açılıştaki "Sözlük endeksleniyor..." sırasında `.qidx` ve `.sidx`
örnekli dizinlerinin kurulması (her 256. girdinin konumu), aramadaki ikili arama ve en çok 256
girdilik tarama, eş anlamlıdan (`.syn`) sıra numarasıyla girdiye gitme, aranan sözcüğün
noktalamadan temizlenip küçültülmesi ve İngilizce kök varyantları (-'s, -’s, -ies, -es, -s, -ed,
-ing) artık yeni `bembeyaz-dict` crate'inde. Dosyalar Rust'a konumdan okuyan bir geri çağrıyla
veriliyor; geri çağrı başarısız konumlamayı (dosya sonunun ötesi ya da bozuk bir küme zinciri)
okuma hatasından ayrı bildiriyor, çünkü taramanın başına konumlanamamak eskisi gibi "Bulunamadı"
değil okuma hatası sayılıyor. Sözcükler artık bayt bayt değil 32 baytlık parçalarla okunuyor ve
eskisi gibi sözlük nesnesindeki 256 baytlık tampona yazılıyor (Rust'ın arama işlevlerinin yığın
çerçevesi 112 ve 144 bayt). Sözlük
klasörünün bulunması, dosyaların açılması, bellek denetimleri, dictzip ve tanımın okunması C++'ta
kaldı. Sözlük kodunun daha önce hiç testi yoktu: eski kod olduğu gibi `test/dictionary`'ye
kopyalandı ve gerçek `Dictionary.cpp` ile aynı bellek içi dosya sistemi üzerinde karşılaştırılıyor.
1000 rastgele sözlükte (rastgele ve 255 bayttan uzun başlık sözcükleri, eş anlamlılar, `.ifo`
çeşitleri, kesik `.idx`, bozuk konumlar, bozulmuş, kesilmiş ya da silinmiş dizinler, okuma hataları
ve bir konumdan sonra konumlanamayan bozuk kümeler) her biri 60 aramayla (büyük/küçük harf, tırnak
ve noktalama, İngilizce ekler, uydurma ve çok uzun sözcükler) dizin dosyaları, endeksleme sonucu
ve her aramanın sonucu, tanımı ve başlık sözcüğü aynı. Koda bilerek konan 24 hatanın 21'i
yakalandı (12'si bu testle, 9'u crate testleriyle); 3'ü sonucu değiştirmiyor. Eski koddaki varyant
tekrarı denetimi hiç tetiklenemediği için kaldırıldı (bir crate testi bunu sabitliyor). Okuma
hatalarında bir fark kalabilir: SD kartta 32 baytlık bir okuma bozuk bir sektöre değerse bütünüyle
başarısız oluyor; eski bayt bayt okuma bozuk sektörden önce biten bir girdiyi yine bulabiliyordu.
`.dict.dz` sözlüklerinin tanım okuması (dictzip) bu testte yok, o yol değişmedi. Emülatörde sonradan `emu/dictionary.txt` (`emu/make_dictionary_cards.py`'nin düz ve
HTML/dictzip kartlarıyla: kökler, eş anlamlılar, tırnaklı ve kesme işaretli sözcükler, 16 KiB'yi
aşan tanım, bulunamayan sözcük) dört sözlük değişikliğinden önceki sürümle (da4da51c) piksel piksel
aynı çıktı. Flash x4pro'da 2152, x4c'de 2156 bayt büyüdü, statik RAM değişmedi.

- [ ] Daha önce endekslenmemiş bir StarDict sözlüğü ayarlayın (Sözlük; ya da klasördeki `.qidx` ve
  `.sidx` dosyalarını silin) ve bir kitapta Sözlükte Ara ile sözcükler arayın: ilk aramada "Sözlük
  endeksleniyor..." görünmeli, sonra tanımlar eskisi gibi gelmeli.
- [ ] Eski sürümün endekslediği bir sözlük yeniden endekslenmeden açılmalı (dizin biçimi
  değişmedi).
- [x] Tırnak içindeki, büyük harfle başlayan ve çoğul ya da -ed/-ing ekli sözcükler kök hâlleriyle
  bulunmalı; sözlükte olmayan bir sözcük "Bulunamadı" demeli.
- [x] `.syn` dosyası olan bir sözlükte bir eş anlamlı (ör. düzensiz bir biçim) aranınca ana
  girdisi gelmeli.
- [ ] Hem `.dict` hem `.dict.dz` sözlüklerde aramalar eskisi kadar hızlı ya da daha hızlı olmalı;
  özellikle FAT32 kartta büyük (`.idx` 20 MB'tan büyük) bir sözlükte, bulunan sözcüklerle birlikte
  kök varyantlarıyla da bulunamayan sözcükleri deneyin (her girdi ayrı konumdan okunuyor; küme
  sınırında geri sarma FAT zincirini baştan yürütebilir).

## Sözlük HTML tanımlarının XHTML'e çevrilmesi Rust'ta ("rust: our own dictionary XHTML")

HTML ya da XDXF tanımlı (`.ifo`'da `sametypesequence=h` veya `x`) StarDict sözlüklerinde tanım
(ve OPDS kataloğundaki kitapların ve eklenti kataloğundaki HTML öğelerin Details (Ayrıntılar)
ekranındaki açıklama), biçimli sayfalara
dökülmeden önce bölüm ayrıştırıcısının okuyabileceği düzgün XHTML'e çevriliyor:
`<html><body>` içine alınıyor, etiket adları küçültülüyor (15 karakterde kesiliyor), boş öğeler
kapatılıyor (`<br>` → `<br/>`) ve kapanışları (`</br>`) atılıyor, yorumlar, doctype ve işleme
yönergeleri atılıyor, işaretleme ya da varlık başvurusu olmayan `<` ve `&`, `&lt;` ve `&amp;` olarak yazılıyor. Bu dönüşüm
artık `bembeyaz-html`'in `xhtml` modülünde; geçici dosyaya 128 baytlık tamponla yazma, yığın
denetimleri ve sayfalama C++'ta. Metin parçaları artık karakter karakter değil, bir sonraki
işaretlemeye ya da kaçışla yazılan karaktere dek bir seferde yazılıyor. Eski işlev olduğu gibi `test/dict_html`'e kopyalandı:
50000 rastgele etiket çorbasında (her türlü etiket, tırnaklı öznitelik, kapanmamış etiket, yorum,
varlık, büyük harf, 15 karakterden uzun ad, UTF-8, rastgele bayt; kimi zaman yazma yarıda kesilerek)
yazılan dosya ve sonuç aynı. Koda bilerek konan 17 hatanın 16'sı yakalandı; kalanı sonucu
değiştirmiyor (boş öğelerin kapanışları zaten erkenden atılıyor). Emülatörde sonradan `emu/dictionary.txt` (`emu/make_dictionary_cards.py`'nin düz ve
HTML/dictzip kartlarıyla: kökler, eş anlamlılar, tırnaklı ve kesme işaretli sözcükler, 16 KiB'yi
aşan tanım, bulunamayan sözcük) dört sözlük değişikliğinden önceki sürümle (da4da51c) piksel piksel
aynı çıktı.
Flash x4pro'da 428, x4c'de 432 bayt büyüdü, statik RAM değişmedi.

- [x] HTML tanımlı bir StarDict sözlüğünde (ör. Wiktionary'den üretilmiş) Sözlükte Ara ile
  sözcükler arayın: tanımlar eskisi gibi biçimli (kalın, italik, listeler, satır sonları)
  görünmeli; düz metne düşmemeli.
- [ ] OPDS kataloğunda HTML açıklamalı bir kitabın Details (Ayrıntılar) ekranını ve oradaki Details
  düğmesiyle açılan tam açıklamayı açın: açıklama eskisi gibi biçimli görünmeli, düz metne
  düşmemeli.

## HTML'in düz metne çevrilmesi Rust'ta ("rust: our own HTML plain text")

Biçimli sayfalara dökülemeyen HTML sözlük tanımları (16 KiB'den uzun ya da biçimli yerleşimi
başarısız olan) ve Details (Ayrıntılar) açıklamaları düz metin olarak gösteriliyor: etiketler
atılıyor, `p`, `h1`–`h6` ve `hr` paragraf, `br`, `div`, `li` ve `tr` satır sonu oluyor (üst üste
binmeden; Details'te satır sonları sonra boşluğa çevriliyor), sekme ve CR tek boşluk, adlı ve
sayısal varlıklar çözülüyor, sondaki boşluk ve satır sonları atılıyor. Bu dönüşüm artık
`bembeyaz-html`'in `plain` modülünde. Metin hiç uzamadığı için (her varlık değerinden uzun; her
satır sonu da yerini aldığı etiketten ya da satır sonundan uzun değil) Rust, C++'ın girdi boyunda
ayırdığı dizeye yazıyor; eskisi de girdi boyunda yer ayırıyordu. Mevcut 6 test değişmeden geçiyor;
eski işlev olduğu gibi `test/html_to_plain_text`'e kopyalandı ve 50000 rastgele parçada (etiketler,
büyük harf ve boşluklu adlar, adlı ve sayısal varlıklar, sınır dışı ve vekil kod noktaları, sekme,
CR, satır sonu, UTF-8, rastgele bayt) çıktı aynı ve hiçbir zaman girdiden uzun değil. Koda bilerek
konan 16 hatanın hepsi yakalandı. Emülatörde sonradan `emu/dictionary.txt` (`emu/make_dictionary_cards.py`'nin düz ve
HTML/dictzip kartlarıyla: kökler, eş anlamlılar, tırnaklı ve kesme işaretli sözcükler, 16 KiB'yi
aşan tanım, bulunamayan sözcük) dört sözlük değişikliğinden önceki sürümle (da4da51c) piksel piksel
aynı çıktı. Flash x4pro'da 384, x4c'de 372 bayt büyüdü, statik RAM değişmedi.

- [x] HTML tanımlı bir sözlükte çok uzun (16 KiB'den büyük) bir tanımı olan bir sözcüğü arayın:
  tanım düz metin olarak, eskisi gibi satır ve paragraflara bölünmüş ve varlıkları (ör. `&amp;`,
  `&#233;`) çözülmüş görünmeli.
- [ ] OPDS kataloğunda HTML açıklamalı bir kitabın Details (Ayrıntılar) ekranı bellek azken (ör.
  büyük bir kitap okunurken açılan katalogda) düz metne düşerse açıklama eskisi gibi tek parça,
  etiketsiz ve varlıkları çözülmüş görünmeli.

## Dictzip başlığı ve parça planı Rust'ta ("rust: our own dictzip tables")

`.dict.dz` sözlüklerinde (dictzip: bağımsız sıkıştırılmış parçalar) gzip başlığının ve `RA` parça
tablosunun okunması (FEXTRA alanları, sürüm, parça boyu ve sayısı, en çok 8192 parça, alan
uzunluklarının tutması, ad, açıklama ve başlık CRC'si, sondaki ISIZE) ile bir tanımın hangi
parçalardan, her birinin ne kadarı atlanıp ne kadarı alınarak okunacağı artık `bembeyaz-dict`'in
`dictzip` modülünde. Parça tablosunun belleği ve öncesindeki yığın denetimi, parçaların açılması
(InflateStream) ve dosya C++'ta. Mevcut 8 DictZip testi gerçek bir sözlük dosyasıyla değişmeden
geçiyor; eski başlık okuyucusu olduğu gibi, parça döngüsü yalnızca parçanın açılması bir geri
çağrıya çevrilerek `test/dict_zip`'e kopyalandı ve 20000
rastgele başlıkta (bozuk sihirli bayt, eksik FEXTRA, başka alanlar, iki `RA`, yanlış sürüm, sıfır ya
da 8192'yi aşan parça sayısı, tutmayan uzunluklar, ad, açıklama, CRC, sıfır ISIZE, kesik dosya,
okuma hatası, parça tablosuna yetmeyen bellek) sonuç, hata türü ve tablo aynı; geçerli başlıklarda
70 binden fazla aralığın parça planı (atlanan ve alınan bayt, kimi zaman bir parçanın açılması
başarısız olarak) da aynı. Koda bilerek konan 24 hatanın 19'u yakalandı; kalan 5'i sonucu
değiştirmiyor (ulaşılamayan ya da gereksiz dallar). Emülatörde sonradan `emu/dictionary.txt` (`emu/make_dictionary_cards.py`'nin düz ve
HTML/dictzip kartlarıyla: kökler, eş anlamlılar, tırnaklı ve kesme işaretli sözcükler, 16 KiB'yi
aşan tanım, bulunamayan sözcük) dört sözlük değişikliğinden önceki sürümle (da4da51c) piksel piksel
aynı çıktı. Ayrıştırma başarısız olursa yarım parça tablosu artık hemen bırakılıyor. Crate'in dictzip için de
mini-fuzz testi var. Flash x4pro'da 528, x4c'de 520 bayt büyüdü, statik RAM değişmedi.

- [x] `.dict.dz` sıkıştırılmış bir StarDict sözlüğünde Sözlükte Ara ile birkaç sözcük arayın
  (kısa ve uzun tanımlar, sözlüğün başından ve sonundan sözcükler): tanımlar eskisi gibi gelmeli.

## Çerçeveli kutular Rust'ta ("rust: our own framed boxes")

Bölüm ayrıştırıcısı kenarlıklı ya da gölgeli (açık gri arka planlı) bir öğeyi bir çerçeve kutusuyla
(`PageBorderBox`) çevreler: kenarlık öğenin iç boşluğuna katılır ve yazı çerçeveden en az 4 piksel
uzak tutulur; iç içe en çok 8 kutu çerçevelenir, 8 pikselden dar kutu çerçevelenmez. Kutu birden
çok sayfaya yayılırsa her sayfada kendi parçası çizilir ve sayfa sonunda kesilen kenarlar açık
kalır; içi boş bir kutu (kenarlıkla çizilen bir çizgi gibi) iç boşluğu kadar yer kaplar. Açık
kutuların yığını ve bu hesaplar artık `bembeyaz-layout`'un `boxes` modülünde (ABI'si yeni
`boxes.h`); yığın ayrıştırıcının içindeki 228 baytlık opak depoda duruyor (eskiden heap'te ayrılan
bir vektördü). `PageBorderBox` nesnelerini kurmak, içi boş kutuyu sayfaya yerleştirmek ve boş
bloğun boşluklarını ona vermek C++'ta kaldı. Sayfa kırılınca kesilen kenarları sabitleyen yeni bir
test eklendi; toplam 1064 host testi geçiyor. Bölüm ayrıştırıcısının fark testine gölgeli, çift,
kesikli, noktalı, tek kenarlı, dar ve kalın iç boşluklu kutular eklendi; 1600 rastgele bölümde eski
ve yeni sayfalar bayt bayt aynı. Koda bilerek konan 16 hatanın 11'ini bu test, kalan 5'ini crate
testleri yakaladı. Emülatörde CSS kitabı (çerçeveli ve gölgeli kutular) okuma ve kitap açma
senaryolarında bir önceki sürümle piksel piksel aynı. Flash x4pro'da 484, x4c'de 416 bayt büyüdü,
statik RAM değişmedi.

- [x] Kenarlıklı ya da arka planı gri kutuları olan bir EPUB'da (ör. not ya da alıntı kutuları)
  kutunun birden çok sayfaya taştığı bir yere gelin: çerçeve her sayfada eskisi gibi çizilmeli,
  sayfa sonunda kesilen üst ve alt kenarlar açık kalmalı. Kenarlıkla çizilmiş içi boş bir çizgi
  sayfanın en altına denk gelirse sonraki sayfada eskisi gibi çizilmeli.

## Büyük ilk harfin yerleşimi Rust'ta ("rust: our own drop cap placement")

Bölüm ayrıştırıcısı bir paragrafın ilk harfini büyük ilk harf (drop cap) yapabilir: CSS'te
`::first-letter`'ın `initial-letter`'ı, en az 1,5 em yazı boyutu ya da `float: left`'i varsa, ya da
paragrafı böyle bir span açıyorsa. Harfin kaç satıra yayılacağı (`initial-letter` 2'den küçükse hiç;
yazı boyutu kadar, 2 ile 4 arası; boyutsuz bir float için 3) artık `bembeyaz-css`'in `inline`
modülünde. Harfin büyütme oranı (üstü ilk satırın büyük harfleriyle, taban çizgisi son satırın
taban çizgisiyle hizalanır; 1 ile 16 kat arası), genişliği (ardındaki bir boşlukla birlikte, en
çok yerin yarısı; yanındaki satırlar bu kadar sağa kayar) ve taban çizgisinin yeri de
`bembeyaz-layout`'un yeni `drop_cap` modülünde (ABI'si yeni `drop_cap.h`); yazı tiplerinin glif
ölçüleri geri çağrıyla gelir, bir yazı tipinde 'H' yoksa eskisi gibi tahmin edilir. Yazı tipini
seçmek, sayfayı kırmak, `PageDropCap`'i kurmak ve yanındaki satırları dizmek C++'ta kaldı.
Geometriyi sabitleyen yeni bir test eklendi; toplam 1065 host testi geçiyor. Bölüm ayrıştırıcısının
fark testi, bu kez glif ölçüleri harfe ve yazı tipine göre değişen (kimi glifleri eksik) bir sahte
çiziciyle, 11600 rastgele bölümde eski ve yeni sayfaları bayt bayt aynı buldu. Koda bilerek konan
19 hatanın 11'ini bu test, kalan 8'ini crate testleri yakalıyor (3'ü için test eklendi). Emülatörde
büyük ilk harfli CSS kitabı okuma ve kitap açma senaryolarında bir önceki sürümle piksel piksel
aynı. Flash x4pro'da 280, x4c'de 284 bayt büyüdü, statik RAM değişmedi.

- [x] CSS'inde `::first-letter` ya da `initial-letter` olan (ya da büyük ilk harf span'ı kullanan)
  bir kitap açın, yazı boyutunu ve yazı tipini değiştirerek birkaç kez bakın: büyük ilk harf eskisi
  gibi büyüklükte, üstü ilk satırın büyük harfleriyle, tabanı son satırın tabanıyla hizalı ve
  yanındaki satırlar eskisi gibi girintili olmalı. Sayfanın altına denk gelen büyük ilk harfli bir
  paragraf harfiyle birlikte sonraki sayfaya geçmeli.

## Resim boyutu Rust'ta ("rust: our own image display size")

Bölüm ayrıştırıcısı bir resmi sayfaya koyarken boyutunu şöyle bulur: CSS'te `width` ve `height`
ikisi de varsa ikisini (em, rem, nokta ya da piksel; yüzde genişlik kapsayıcı bloğun, yüzde yükseklik
sayfanın) alır ve kapsayıcıya ya da sayfaya sığmazsa ikisini aynı oranda küçültür; yalnızca biri
varsa öbürünü resmin oranından bulur ve sığmayan kenarı sınırlar; CSS'te boyut yoksa resmin kendi
boyutunu sığacak kadar küçültür. Bu hesap artık `bembeyaz-css`'in yeni `image` modülünde. Resmin
boyutunun dosyadan okunması, kapsayıcının genişliği, sayfanın kırılması ve `PageImage`'ın kurulması
C++'ta kaldı. Eski hesap olduğu gibi `test/css_parser/OldImageSize.h`'e kopyalandı ve 200 000
rastgele durumda (her birimde, sıfır, negatif, kesirli ve aşırı büyük CSS değerleri; sıfır, negatif
ve 32767'ye kadar resim boyutları; tanımsız bit ile saklanmış uzunluklar) sonuç aynı; toplam 1067
host testi geçiyor. Koda bilerek konan 17 hatanın 16'sını bu test yakaladı; kalanı sonucu
değiştirmiyordu ve o gereksiz denetim kaldırıldı. Emülatöre CSS ile boyutlanmış resimlerden oluşan
yeni bir kitap (`jpeg-sized.epub`, senaryosu `emu/jpeg-sized.txt`) eklendi; bu senaryo ile JPEG ve
PNG senaryoları bir önceki sürümle piksel piksel aynı. Flash x4pro'da 512, x4c'de 444 bayt büyüdü,
statik RAM değişmedi.

- [x] Resimlerine CSS ile genişlik ya da yükseklik veren bir EPUB'da (ör. `width: 50%` ya da
  `height: 10em`) resimli sayfalara bakın: resimler eskisi gibi büyüklükte ve oranları bozulmadan
  görünmeli, sayfadan taşmamalı.

## Sayfa sonunda taşınan bağlantılar ve dipnotlar Rust'ta ("rust: our own carried links and footnotes")

Bölüm ayrıştırıcısı bir sayfayı kırarken son satırlarından bazılarını sonraki sayfaya taşıdığında
(paragrafın yalnız kalan satırları, başlıklar) o satırlardaki bağlantılar da taşınır: taşınan
satırların üstünün altına uzanan bağlantılar sonraki sayfaya geçer, gösterdikleri dipnotlar da
sonraki sayfanın dipnotları olur; bir dipnotu sayfada kalan bir bağlantı da gösteriyorsa dipnot iki
sayfada da yer alır. Bu ayırma artık `bembeyaz-section`'ın yeni `notes` modülünde (ABI'si
`page.h`'ta); bağlantılar ve dipnotlar sırasını korur ve ayırma yerinde, ek bellek ayırmadan
yapılır. Taşınan bağlantıları yeni sayfaya kaydırarak eklemek `Page::moveNotesBelow`'da (eskiden
taşınan bağlantılar için ayrılan ara liste artık yok), satırları taşımak ayrıştırıcıda, C++'ta
kaldı. Toplam
1067 host testi geçiyor. Bölüm ayrıştırıcısının fark testi (iç bağlantılar dipnot üretir) 10 000
rastgele bölümde eski ve yeni sayfaları bayt bayt aynı buldu. Koda bilerek konan 10 hatanın 7'sini
bu test, 2'sini crate testleri yakaladı (birine test eklendi: bağlantılar sayfaya yukarıdan aşağı
eklendiği için taşınanlar zaten sonda durur ve fark testi sıra bozulmasını göremez); kalanı sonucu
değiştirmiyor. Emülatörde dipnotlu CSS kitabı ve heceleme kitabı bir önceki sürümle piksel piksel
aynı. Flash x4pro'da 812, x4c'de 648 bayt büyüdü, statik RAM değişmedi.

- [x] Dipnotlu bir kitapta, dipnot bağlantısı olan bir satırın sayfa sonunda sonraki sayfaya
  taşındığı bir yere gelin (ör. paragrafın son satırı): bağlantıya dokununca (Dokunmatik okuyucu
  kontrolleri açıkken) dipnot eskisi gibi açılmalı; dipnot, taşınan satırın geldiği sayfada
  menüdeki "Bağlantılar ve dipnotlar" listesinde görünmeli, önceki sayfanın listesinde ise yalnızca
  orada da onu gösteren bir bağlantı varsa görünmeli.

## Sürüm 0.0.3 ("release: 0.0.3")

0.0.2'den bu yana: container.xml, BMP, XTC, KOReader konumları, kütüphane (metin, dizin okuma ve
yazma), TXT/Markdown dönüştürme, SD kart fontları (başlık, kerning, glifler, aralıklar, sayfa
kümesi, ilerleme genişlikleri), yerleşik sıkıştırılmış fontlar, bölüm dosyası ve sayfaları, kitap
üst veri önbelleği, alıntı deposu, GfxRenderer'ın çizimleri (temeller, yuvarlak şekiller,
çokgenler, resimler, glifler, ekran kopyaları), resim hattı (titreme, önbellekteki satırlar, JPEG ve
PNG örnekleme, kapak ölçekleme, boyut yoklaması), sözlük (dizin, XHTML, düz metin, dictzip) ve
bölüm ayrıştırıcısının sözcük bölme, stil, yazı boyutu, blok stili, öğe sınıflandırması, sayfa
taşıma, tablo, liste, çerçeveli kutu, büyük ilk harf, resim boyutu ve taşınan dipnot parçaları
Rust'ta; birkaç düzeltme (sayfa bağlantılarının boş baytları, kısa SD font glifleri, alttan
başlayan resimlerin kırpılması, uzun bir öğeden sonraki KOReader konumu) ve alıntı deposunda hız.
Bütün emülatör senaryoları 0.0.2 ile karşılaştırıldı (sonuçlar `rust/TEST-PLANI-0.0.3.md`'de);
cihaz denemesi o plana göre yapılır.

- [x] Hakkında'da Firmware satırı "purestWhite next 0.0.3" olmalı, açılışta altta "0.0.3"; Start
  Other Firmware önceki yazılımı adıyla ("purestWhite next 0.0.2") göstermeli.
- [ ] 0.0.3 GitHub'da yayımlandıktan sonra 0.0.2 çalışan bir cihazda Check for Updates 0.0.3'ü
  önermeli.

## Web yolları Rust'ta ("rust: our own web path checks")

Web dosya yöneticisi, WebDAV ve eklenti uç noktaları gelen yolları şöyle denetler: kimlik bilgisi
dosyalarına (`/.crosspoint/wifi.json`, `opds.json`, `koreader.json` ve bunlarla başlayan geçici ve
yedek dosyalar) hiçbir yazımla ulaşılamaz (`//`, `/./`, `..`, büyük/küçük harf, SdFat'ın kırptığı
baştaki boşluklar ve sondaki nokta ve boşluklar, ilk iki bileşende `CROSSP~1` gibi 8.3 kısa adlar);
nokta ile başlayan adlar ile `System Volume Information` ve `XTCache` değiştirilemez ve silinemez
(listede nokta ile başlayanlar yalnızca Gizli Dosyaları Göster açıkken görünür, sistem adları hiç
görünmez); isteğin yolu mutlak yola çevrilir (`..` uygulanır, başta tek `/`, sonda yok);
WebDAV'ın `Destination` başlığından yol kısmı alınır; yollar URL'de kodlanır (boşluk, `%`, `#`,
`?`, `&` ve ASCII dışı baytlar `%XX`). Bunlar artık `bembeyaz-path`'in yeni `web` modülünde;
web sunucusunda ve WebDAV'da ayrı ayrı tutulan gizli ad listesi tek yerde. Arduino'nun
`urlDecode`'u ve istekleri işlemek C++'ta kaldı. Eski kod `test/web_paths/OldWebPaths.h`'e olduğu
gibi kopyalandı ve 300 000 rastgele yol ve URL'de (eğik çizgiler, `.` ve `..`, kimlik bilgisi ve
sistem adları, boşluk ve noktalı adlar, kısa adlar, `%` kaçışları, şemalar, ASCII dışı baytlar)
bütün denetimlerin sonucu aynı; crate'in mini-fuzz testi de var. Adlar eskisi gibi ilk NUL'e kadar
karşılaştırılır: istekte `%00` ile gelen `XTCache%00x` adı da korunur (incelemede bulundu, ilk
taslakta açıktı; test artık ham NUL baytı da üretiyor). NUL'lü yolda gizli bileşen denetimi NUL'den
sonrasını da bölerek arar, yani eskisinden yalnızca daha katı; URL kodlaması da NUL'de biter (eski kod
NUL'ü ve ardını kopyalıyordu). İkisinde de istekten gelen yollar zaten ilk NUL'de kesilmiş olur. Flash x4pro'da 640, x4c'de 688 bayt
büyüdü, statik RAM değişmedi.

- [ ] Wi-Fi'ye bağlanıp tarayıcıda `http://purestwhite.local` dosya yöneticisini açın: kartın
  kökünde `System Volume Information` ve `XTCache` hiç, `.crosspoint` Gizli Dosyaları Göster
  kapalıyken listelenmemeli; boşluklu ve Türkçe harfli adlı bir klasöre dosya
  yükleyip indirin, yeniden adlandırın ve silin. `curl -X POST -d 'path=/&name=XTCache%00'
  http://purestwhite.local/mkdir` 403 dönmeli ve kökte `XTCache` klasörü oluşmamalı.
- [ ] Bir WebDAV istemcisiyle (ör. Linux'ta Dosyalar → `davs://` yerine `dav://purestwhite.local`,
  ya da `cadaver`) bağlanın: klasörler listelenmeli, boşluklu ve Türkçe harfli adlı bir dosya
  yüklenip taşınabilmeli ve kopyalanabilmeli; `.crosspoint` ve içindekiler görünmemeli ve
  değiştirilememeli.

## Sayfalayıcı Rust'ta ("rust: our own chapter pager")

Bölüm ayrıştırıcısının sayfa imleci (bir sonraki boş satır, sayfa kırılınca sonraki sayfaya
taşınan yalnız kalmış satırlar ve başlıklar, her sayfanın ilk görünür karakteri, çerçeveli
kutular, yatay çizgi, tablo satırı ayırıcısı, büyük ilk harf ve resimlerin yeri) `bembeyaz-layout`'un
yeni `pager` modülünde. Sayfa nesneleri C++'ta kaldı. `test/chapter_corpus`'un 1600 bölümünün her
sayfası baytı baytına aynı; emülatörde bölüm senaryolarının ekranları önceki commit'le aynı.
Bellek yetmezliğinde artık sayfa ya da kutu kurulamazsa bölüm derlemesi okunur bir hatayla duruyor
(eskiden `new Page()` çöküyordu ya da öğe sessizce düşüyordu). Flash x4pro'da 2,4 KB büyüdü
(geçici FFI girişleri; ayrıştırıcının akışı Rust'a geçince kalkacak), statik RAM değişmedi.

- [ ] Uzun bir romanı ve resimli, tablolu bir kitabı açıp birkaç bölüm boyunca sayfa çevirin:
  paragrafın ilk ya da son satırı sayfada yalnız kalmamalı, başlık sayfanın dibinde kalmamalı,
  çerçeveli kutular sayfa kırılınca açık kenarla sürmeli, resimler sayfadan taşmamalı.
- [ ] Bir bölümün ortasındayken kitabı kapatıp açın: aynı sayfaya dönmeli (sayfaların görünür
  konumları); içindekilerden bir bölüme ve bir dipnota atlayın (çapalar).
- [ ] Seri porttan `Page render` sürelerini 0.0.3'le karşılaştırın; fark gürültü düzeyinde olmalı.

## Çapalar ve dipnotlar Rust'ta ("rust: our own chapter anchors and note references")

Bölümdeki kimliklerin düştüğü sayfalar ve dipnot göndermeleri (numara, hedef, gösterildiği satırın
sayfası) yeni `bembeyaz-chapter` crate'inde. Adlar tek bir tamponda: çapa başına eskiden 28 bayt
(`std::string` ve sayfa) artı 15 karakterden uzun adlarda ayrı bir blok, bekleyen dipnot başına 292
bayt gidiyordu; açık bağlantının 288 baytlık kaydı da ayrıştırıcıdan kalktı.
`test/chapter_corpus`'un 1600 bölümü (içindekiler çapaları, dipnot bağlantıları dahil) baytı
baytına aynı; Rust koduna konan 4 hatanın 4'ünü yakaladı. Flash x4pro'da 936, x4c'de 876 bayt büyüdü.

- [ ] Dipnotlu bir kitapta (ör. Standard Ebooks'tan biri) dipnot numarasına dokunup dipnota gidin
  ve geri dönün; sayfanın dipnot listesi doğru numaraları göstermeli.
- [ ] İçindekilerden bir bölümün ortasındaki bir alt başlığa atlayın: doğru sayfa açılmalı;
  içindekilerin başlattığı bölümler yeni sayfada başlamalı.

## Bölüm ayrıştırıcısı Rust'ta ("rust: our own chapter parser")

ChapterHtmlSlimParser'ın akışının tamamı (XHTML olayları, blok ve satır içi stiller, paragrafların
kurulması ve satırlara kırılması, büyük ilk harf, listeler, tablolar, resimler, çerçeveli kutular,
yatay çizgiler, çapalar, dipnotlar ve sayfalara yerleşim) `bembeyaz-chapter`'da. C++'ta yalnızca
dosya okuma, sayfa ve öğe nesneleri (`Page`, `TextBlock`, `PageImage`...), resim boyutunun okunması
ve yazı tipi ölçüleri kaldı; Rust bunları `bb_chapter_host` geri çağrılarıyla ister. Sözlük
tanımları ve Ayrıntılar ekranındaki HTML açıklamalar da aynı ayrıştırıcıdan geçer.
`test/chapter_corpus`'un 1600 bölümü (resimler, tablolar, dipnotlar dahil) baytı baytına aynı;
Rust koduna konan 6 hatanın 6'sını yakaladı. Emülatörde 16 senaryonun ekranları önceki commit'le
aynı. Flash x4pro'da 8,0 KB, x4c'de 7,8 KB büyüdü (yeni Rust kodu, silinen C++ ve FFI
girişlerinden daha büyük), statik RAM değişmedi. Bölüm açılırken derleme 16 KB'lık çizim
görevinde, okurken arka planda süren derleme 8 KB'lık döngü görevinde (`loop()`) çalışır; en
derin çağrı zinciri (metin → tablo hücresi → satır kırma) dökümden hesaplandığında eskisinden
yaklaşık 0,45 KB derin (3,15 KB'a karşı 2,7 KB). Paragraf ve XML
ayrıştırıcısı heap'te tutulur (eskiden de paragraf `unique_ptr` ile heap'teydi). SD okuma
hatası artık derlemeyi durdurur (eskiden geçersiz bir uzunlukla ayrıştırıcıya gidiyordu).

- [ ] Birkaç farklı kitabı (uzun roman, resimli, tablolu, dipnotlu, sağdan sola, heceli, büyük
  ilk harfli) açıp birkaç bölüm boyunca sayfa çevirin; görünüm 0.0.3'le aynı olmalı.
- [ ] Ayarlardan yazı boyutunu, satır aralığını, hizalamayı, paragraf boşluğunu ve girintisini,
  harf ve sözcük aralığını, resim ayarını (resim / alternatif metin / hiçbiri) ve "Kitabın
  Stili"ni değiştirip bir bölümü yeniden açın: sayfalar yeni ayarla kurulmalı.
- [ ] Çok büyük tek bölümlü bir kitabı açın (bölüm arka planda derlenir): ilk sayfalar hemen
  gelmeli, ilerledikçe sayfa sayısı tahmini düzelmeli, okurken çökme olmamalı.
- [ ] Tablosu çok olan büyük bir bölümü (ör. bir sözlük ya da istatistik kitabı) SD kart fontuyla
  açın ve arka plan derlemesi yeniden başlayana dek (derlenmiş sayfaların sonuna yaklaşınca)
  okuyun: derleme yığın taşması ("Stack canary" / "stack overflow") olmadan bitmeli.
- [ ] Sözlükte bir sözcük arayın (HTML tanımlı bir sözlükle) ve HTML açıklamalı bir kitabın
  Ayrıntılar ekranını açın: metin biçimli sayfalarda görünmeli.
- [ ] Seri porttan bölüm derleme ve `Page render` sürelerini 0.0.3'le karşılaştırın; fark gürültü
  düzeyinde olmalı.

## Gerçek zamanlı saat Rust'ta ("rust: our own real-time clock driver")

HAL taşımasının ilk dilimi. `HalClock` saati artık SDK'nın `Rtc` sürücüsüyle değil, yeni
`purestwhite-rtc` crate'iyle okuyup yazıyor (BM8563 / PCF8563: bulunması, BCD saat, gerilim düşük
bayrağı, yüzyıl biti, UTC'den Unix zamanına çevrim). Veriyolu dokunma ve pil göstergesiyle
paylaşıldığı için Arduino `Wire` üzerinde, SDK'nın yaptığı aktarımların aynısıyla kaldı. Yerel saat,
saat dilimi ve NTP değişmedi. `test/rtc_pcf8563` iki sürücüyü SDK'nın kayıt tutan `Wire` taklidinde
karşılaştırıyor (bulma ve yeniden deneme, 20 000 okuma, 20 000 yazma, 200 000 zaman çevrimi; hepsi
aynı, Rust'a konan 5 hatanın 5'ini yakaladı). Emülatörde `emu/clock.txt` başlıkta 09:00'ı, bir dakika
sonra durum çubuğunda 09:01'i gösteriyor; ekranlar önceki commit'le aynı. Flash x4pro'da 1,3 KB,
x4c'de 1,4 KB küçüldü (SDK'nın öteki saat yongalarının kodu artık bağlanmıyor). Açılış günlüğünde
"SDK RTC found" yerine "RTC found" yazıyor.

Adımlar X4 Pro ve X4 Classic için aynı; Türkçe arayüzde bazı etiketler İngilizce kalır ("System",
"Show in Header", "Time Zone").

- [ ] Açılış günlüğünde `[CLK] RTC found` ve doğru tarihli `RTC clock read:` satırları görünmeli.
  Ayarlar → System altında "Saat" girdisi görünmeli: yalnızca saat yongası bulununca eklenir.
- [ ] Ayarlar → System → Saat → "Show in Header"ı ve Ayarlar → Okuyucu → Durum Çubuğunu Özelleştir →
  Saat'i açın: başlıktaki ve kitabın durum çubuğundaki saat doğru olmalı ve ilerlemeli.
- [ ] Ayarlar → System → Saat → "Saati şimdi eşitle" (Wi-Fi kapalıysa ağ seçimi açılır; ekrandan
  çıkınca cihaz sessizce yeniden başlar): saat doğru olmalı; cihazı kapatıp açınca da doğru kalmalı
  (saat yongaya yazıldı).
- [ ] Saat dilimini ("Time Zone") değiştirin: gösterilen saat hemen yeni dilime geçmeli.
- [ ] Alıntı alın: kartın kökündeki `My Clippings.txt` dosyasının son "Added on" satırı bugünün
  tarihi ve saati olmalı.
- [ ] Saat durum çubuğunda açıkken bir süre dokunarak sayfa çevirin: saat güncellenmeye devam
  etmeli, seri portta `i2cWriteReadNonStop returned Error` ya da "Unfinished Repeated Start" satırı
  olmamalı (saat, dokunma ve pil göstergesi aynı veriyolunda).

## Pil göstergesi Rust'ta ("rust: our own fuel gauge driver")

HAL'in ikinci dilimi. `HalPowerManager` pil yüzdesini artık SDK'nın `BatteryMonitor`'ı yerine yeni
`purestwhite-gauge` crate'iyle okuyor (CW2017: ilk okumada 80 baytlık pil profilinin denetlenmesi
ve gerekirse yüklenmesi, yumuşak sıfırlama, hesaplama motorunun hazır olmasını bekleme, uykuda ya
da yeniden başlamış bulunan göstergenin bir sonraki okumada toparlanması, başarısız ilk okumadan
sonra bir saniye bekleme). Sürücüler artık embedded-hal'in trait'leriyle yazılıyor: saat `I2c` ve
`DelayNs`, pil göstergesi (ilk okuması 4 saniyeye kadar bekleyebildiği için)
embedded-hal-async'inkilerle; ikisi bare-metal firmware'de esp-hal üstünde değişmeden çalışacak.
Pil okuması artık bir kilitle tek seferde bir görevden yapılıyor (çizim görevi ve ana görev ikisi
de okuyor). Şarj durumu (GPIO21) değişmedi. `test/cw2017_gauge` iki sürücüyü 30 000 okumalık tek
bir oturumda, aynı hataları ve bozulmaları (uyku, yeniden başlama, kaybolan profil, geçersiz yüzde,
tam dolu) gören ikiz taklit yongalarla karşılaştırıyor: her okumanın sonucu, yazmaları, işlem
sayısı ve beklemesi aynı; Rust'a konan 9 hatanın 9'unu yakaladı. Emülatörde `emu/battery.txt`
yüzdeyi %80'den %42'ye ve %100'e çevirip başlıkta gösteriyor; ekranlar önceki commit'le aynı. Flash
x4pro'da ve x4c'de 2,3 KB büyüdü (async sürücünün durum makinesi 1,9 KB); SDK'nın CW2017 yolu öteki
kartlar için yedek olarak bağlı kaldığı için küçülme yok. Saat de yeni köprüden geçiyor: önceki
bölümün saat adımları bunu da sınar.

Adımlar X4 Pro ve X4 Classic için aynı; iki kart da SDK'nın X4 Pro hücresi için kaydettiği profili
alır (eskisi gibi), X4 Classic'teki yüzdeyi bir kez özgün yazılımla karşılaştırmak iyi olur.
Göstergenin uykudan toparlanması ve profilin yeniden yüklenmesi kullanıcı tarafından tetiklenemez
(pil tamamen bitmedikçe ya da sökülmedikçe); onlar `test/cw2017_gauge` ve emülatörle sınanıyor.

- [ ] Açılıştan sonra başlıktaki pil yüzdesi gerçekçi olmalı (%0 değil). Gösterge yeniden
  başlatılırsa ilk ekran 4 saniyeye kadar gecikebilir; ekran gelince düğmeler hemen yanıt vermeli.
- [ ] Bir süre okuyun: yüzde yavaşça düşmeli; şarja takınca yükselmeli, şarj simgesi de çıkmalı.
- [ ] Cihazı derin uykudan uyandırın (uyanmak bir yeniden açılıştır: ilk okuma profili denetler,
  profil zaten doğruysa hiçbir şey yazmaz): yüzde hemen doğru görünmeli.
- [ ] Ayarlar → Ekran → Pil Yüzdesini Gizle → "Asla" seçiliyken okuyucunun durum çubuğunda yüzde
  görünmeli; dokunarak sayfa çevirirken seri portta `i2cWriteReadNonStop returned Error` satırı
  olmamalı (pil göstergesi, saat ve dokunma aynı veriyolunda).

## Bare-metal ilk deneme ("rust: a bare-metal smoke test for the second OTA slot")

Planın 2. aşamasının ilk adımı: `rust/firmware`, esp-hal ve Embassy üstünde ayrı bir bare-metal
firmware. Şimdilik bir duman testi: ikinci OTA yuvasında bir kez açılır, ilk iş sonraki açılışı
C++ firmware'e geri verir (otadata), sonra esp-rtos ve Embassy'yi başlatır, saat ve pil
sürücülerimizi (`purestwhite-rtc`, `purestwhite-gauge`) esp-hal'in gerçek I2C'si üzerinde
değiştirmeden çalıştırır (ilk sürümler Embassy zamanlayıcısını saat yongasıyla da
karşılaştırırdı), sonucu RTC yavaş belleğine bırakıp sıfırlanır. C++ firmware açılışta bu raporu
kartın köküne `purestwhite-bare-metal.txt` olarak yazar (`HalSystem::saveBareMetalReport`). Geri
dönüş denemesi ekrana hiçbir şey çizmez: cihaz SD güncellemesinden sonra birkaç saniye eski ekranla
durur. Duman testi artık dokunma penceresinde ekranı siyaha, sonra beyaza boyar (bkz. "Bare-metal
dokunma"). Sonra C++ firmware yeniden açılır, en çok bir dakika içinde.

Güvenlik: 60 saniyelik RTC bekçisi en başta kurulur ve her takılmayı sıfırlar; ardından sonraki
açılış C++ firmware'e, yani imajın açıldığı yuvanın (MMU'dan okunur) ötekine verilir (önyükleyicinin
geri alma ayarına güvenilmez; C++ firmware ota_0'da da ota_1'de de olabilir); ikinci çekirdek park
edilir; panik rapora "PANIK" yazar ve hemen sıfırlar. Rapor her aşamadan sonra yeniden kaydedilir,
yarıda kalan bir çalışma nereye kadar geldiğini söyler. Emülatörde (`emu/baremetal-handback.txt`) SD
güncellemesi → bare-metal açılış → C++'a dönüş → rapor → bir sıfırlama daha, baştan sona geçiyor,
C++ firmware ota_1'deyken de (`emu/baremetal-handback-slot1.txt`); planlı bir panik de C++'a
dönüyor. esp-rtos ise eink-qemu 0.9.2'de ilk görev değişiminde emülatörü çökertiyor (bkz.
`EMULATOR-HAL.md`), duman testinin kendisi bu yüzden yalnızca cihazda denenebilir.

Önce C++ firmware'in bu sürümü kurulu olmalı (raporu o yazar); deneme imajları onun öteki yuvasına
yazılır ve oradaki önceki firmware'in üstüne yazar (Ayarlar'daki "Diğer yazılımla başlat" artık
önceki sürümü değil deneme imajını açar). Dosyalar `~/Documents/bembeyaz/firmware-arsiv/`'de
(sağlama toplamları yanlarında).

**USB'de veri aktarımı yok, yani cihazı USB'den kurtarmak mümkün değil.** Bare-metal imajlar yalnızca
önyükleyici geri alıyorsa denenir. Geri alan bir önyükleyici, kendini onaylamayan bir imajdan
sonraki ilk sıfırlamada önceki firmware'e kendisi döner; imaj hiç çalışmasa bile. Geri almayan bir
önyükleyicide dönüşü yalnızca bare-metal imajın kendisi yapabilir; bir hata cihazı kalıcı olarak
bozar.

- [x] Kartta `.crosspoint/onyukleyici.txt` varsa silin (gizli klasör). Bu C++ firmware'i
  (`purestWhite-next-bare-metal-rapor.bin`) her zamanki gibi SD ile kurun. Açılınca kartı
  bilgisayarda açıp `.crosspoint/onyukleyici.txt`'yi okuyun ve bana iletin. Bu adımın riski her
  zamanki SD güncellemesiyle aynıdır. "onyukleyici geri alma: VAR" yazıyorsa aşağıdaki adımlara
  geçilebilir; "YOK" ya da "BILINMIYOR" yazıyorsa ya da dosya yoksa **durun, bare-metal imajları
  denemeyin**.
- [x] `purestwhite-baremetal-geri-donus.bin`'i kartın köküne koyun; Ayarlar → System (bu başlığın
  Türkçesi yok) → SD Karttan Firmware Güncelleme → dosyayı seçin → Onayla. Cihaz yeniden başlamalı,
  birkaç saniye içinde C++ firmware'e dönmeli. `purestwhite-bare-metal.txt`'yi kartı bilgisayara
  takıp okuyun (cihazdaki okuyucu aynı boydaki eski bir raporu önbellekten gösterebilir): "geri
  donus denemesi", "sonraki acilis: C++ yazilimi", "onyukleyici durumu: N" ve "bitti" yazmalı. N'yi
  bana iletin (1: önyükleyici kendisi de geri alıyor, 0: almıyor; "kayit yok" ya da başka bir
  sayı da bilgi verir). Cihazı bir kez daha kapatıp açın: C++ firmware açılmalı.
  - Sonuç (2026-10-26, cihaz): önyükleyici yoklaması "VAR (yuva app1, durum 2)". Geri dönüş
    denemesi de64e334 öncesi iki kez raporsuz bitti (cihaz kapandı, yukarıdaki nota bakın);
    de64e334 ile: "sonraki acilis: C++ yazilimi", "onyukleyici durumu: 1", "bitti", cihaz
    kendiliğinden C++'a döndü. Raporlar `firmware-arsiv/cihaz-raporlari/`'nda.
- [x] Aynısını `purestwhite-baremetal-deneme.bin` ile yapın. Dosyanın tamamını bana iletin:
  "esp-rtos: basladi", saat (UTC, doğru olmalı), pil yüzdesi (başlıktakine yakın),
  "zamanlayici: 10 s = saat 10 s" (9–11 s olağan) ve "bitti" bekleniyor; "PANIK"
  ya da eksik satırlar nerede durduğunu gösterir.
- İlk iki denemede rapor gelmedi, büyük olasılıkla şundan: imaj sistem sıfırlamasıyla bitiyordu, bu sıfırlama GPIO1'i
  bıraktı ve pildeki cihaz kapandı (güç kesilince RTC'deki rapor da silinir). İmajlar artık GPIO1'i
  pin tutmayla (pad hold) sıfırlamadan geçirir; bekçi de RTC bölgesine dokunmayan sıfırlamayı
  kullanır.
  - Sonuç (2026-10-26, cihaz, ilk deneme): "esp-rtos: basladi", "pil: %81", "bitti"; saat
    "bulundu, okunamadi", zamanlayıcı "saat yok". Saat sürücüsü VL bayrağında C++'taki gibi
    zaman vermez; cihazın saati muhtemelen hiç ayarlanmadı (karttaki dosyalar 2026-01-01
    tarihli). Rapor artık nedeni ayırır ve zamanlayıcıyı ham saniyelerle ölçer.
  - Sonuç (4cef0e27 ile ikinci deneme): "saat: ayarlanmamis (VL biti)", "pil: %80",
    "zamanlayici: 10 s = saat 10 s", "bitti". Saatin ayarlanmadığı doğrulandı; Embassy'nin
    zamanlayıcısı saat yongasıyla uyuşuyor.
- [ ] Deneme sırasında cihazı kapatmayın: rapor RTC belleğinde bekler ve güç kesilince silinir.
  Ana ekran gelene kadar (en çok 60 saniye) dokunmadan bekleyin; "Bare-metal dokunma"
  denemesinde yalnızca ekran siyahken dokunun.
- [ ] Her denemeden önce eski `purestwhite-bare-metal.txt`'yi silin. Dosya oluşmadıysa imaj
  açılmamış ya da raporu bırakamadan sıfırlanmış demektir; bana haber verin.
- [ ] Bir adım cihazı geri getirmezse: en çok 60 saniye bekleyin (bekçi sıfırlar). Cihaz C++
  firmware'e hiç dönmüyorsa Aşağı+Güç kurtarma kipi de açılmaz (o kip C++ firmware'in içinde) ve
  Güç düğmesi de bir şey yapmaz (firmware'in okuduğu bir giriş); USB'den de kurtarılamaz; durup
  bana haber verin.

## Bare-metal dokunma ("rust: our own GT911 touch driver, in the bare-metal firmware", "rust: show the bare-metal touch window on the panel")

`purestwhite-gt911`, X4 Pro'nun GT911 dokunmatik yongasının sürücüsü; şimdilik yalnızca bare-metal
deneme imajında çalışır (C++ firmware'de dokunma SDK'nın InputManager'ında kalır). İmaj, saat ve
pilden sonra yongayı resetler ve adresini bulur; yonga bulunduysa ekran denetleyicisini yoklar ve
UC8279 ya da UC8179'da ekranı tamamen siyaha boyar (`purestwhite-epd`, "Bare-metal ekran"daki
sürücü). Ekranın siyah kaldığı yaklaşık 15 saniye boyunca dokunuşları her 20 ms'de bir okur, sonra
ekranı beyaza boyar, denetleyiciyi uyutur ve C++'a döner. SSD1677'de ekran değişmez, pencere
görünmez. Ekrana çizdiği için yalnızca X4 Pro'da denenir (X4 Classic'te değil). Geri dönüş
"Bare-metal ilk deneme"dekiyle aynıdır; önyükleyici geri alma VAR olmalı.

- [x] Kartta eski `purestwhite-bare-metal.txt` varsa silin. `purestwhite-baremetal-deneme.bin`'i (bu
  commit'teki imaj; arşivdeki sağlama toplamıyla) koyun; Ayarlar → System → SD Karttan Firmware
  Güncelleme → dosyayı seçin → Onayla. Güncellemeden birkaç saniye sonra ekran birkaç kez yanıp
  söner ve siyah kalır. Yalnızca ekran siyahken, şu sırayla dokunun: önce ekranın sol üst köşesine
  birkaç kez, sonra Home tuşuna iki kez, sonra iki parmakla birkaç kez, en son sağ alt köşeye
  parmağınızı koyup ekran beyaza dönmeye başlayana kadar orada tutun. Ekran beyaz olduktan birkaç
  saniye sonra C++ ana ekranı gelmeli. Ekran siyaha hiç dönmezse dokunmadan bekleyin; C++ en çok
  bir dakikada gelir.
- [x] Raporda "dokunma: 0x5d" (ya da 0x14), "ekran UC8179: siyah tamam T s" (ya da UC8279),
  "kare N cok K ilk X,Y son X,Y home H hata E", "beyaz tamam T s" ve "bitti" bekleniyor: N
  sıfırdan büyük, K en az 2, H en az 1, E 0 ya da çok küçük. "ilk" sol üst, "son" sağ alt köşe
  olmalı; iki noktanın X'leri ve Y'leri birbirinden belirgin farklı olmalı (değerleri bana iletin,
  eksen yönünü doğrular). T, yenilemenin saniyesi: birkaç saniye olağan, 8 s'ye yakınsa sınır dar
  demektir (bana iletin). Ekran siyaha dönmediyse "ekran" satırı nedenini söyler (SSD1677'de
  "atlandi"; dokunma yongası bulunmadıysa "ekran" satırı hiç yazılmaz); pencere yine 15 saniye açık
  kalır. "siyah hata" ya da "beyaz hata" T 8 s civarındaysa denetleyici o yenilemeyi 8 saniyede
  bitirmemiş (BUSY boşalmamış), kısaysa veriyolu hata vermiştir; ondan sonra panele bir şey
  gönderilmez ("beyaz tamam T s, uyku hata": yalnızca uyutma). Bir satır yarım kaldıysa
  ve C++ bir dakika kadar sonra geldiyse imaj orada takılmış, bekçi sıfırlamıştır. "sonraki acilis:
  SECILEMEDI (...)" yazıp başka satır yoksa imaj açılışı C++'a veremediği için hiçbir şeye
  dokunmadan sıfırlanmıştır (dönüşü önyükleyicinin geri alması sağlar): bunu bana iletin. Satır
  sonunda `~` varsa rapor sığmamış demektir.
  - Sonuç (2026-10-26, cihaz, ilk deneme): "dokunma: 0x5d", "kare 71 cok 1 ilk 794,467 son
    794,470 home 0 hata 0", "zaman: 15 s = saat 15 s". Sürücü yongayı buldu ve 15 s hatasız okudu;
    pencere görünmediği için o sırada dokunulmadı (dokunuşlar C++ ana ekranı geldikten sonra),
    kenardaki sabit temas büyük olasılıkla tutan el. Home ve iki parmak bare-metal'de henüz
    sınanmadı: pencere görünür olmadığı için imaj artık onu ekranı siyaha boyayarak gösteriyor.
    Zamanlayıcı ile saat iki denemede de uyuştuğu için "zaman" satırı, "esp-rtos: basladi" ile
    birlikte, ekran satırlarına yer açmak için rapordan çıktı.
  - Sonuç (2026-10-26, 715ac43b'nin imajı, ikinci deneme): "ekran UC8179: siyah tamam 1.9 s",
    "beyaz tamam 1.9 s": Rust ekran sürücüsü gerçek UC8179'da ilk kez çizdi (ekran siyah oldu,
    sonra beyaz), tam yenileme 1,9 s, 8 s sınırına bol pay. "kare 3 cok 0 ilk -1,-1 son -1,-1
    home 0 hata 0": pencerede hiç temas gelmedi; sahibi ekran siyahken bir kez dokunmuş, büyük
    olasılıkla siyah yenileme bitmeden (pencere yenileme bitince açılır), sonra ekran beyaza
    dönmüş. Deneme yukarıdaki sırayla, ekran siyahta durduktan sonra yeniden yapılacak.
  - Sonuç (2026-10-26, üçüncü deneme, aynı imaj): "siyah tamam 1.9 s", "kare 543 cok 2 ilk 57,462
    son 710,39 home 14 hata 0", "beyaz tamam 1.9 s". Sürücü 15 s boyunca hatasız okudu; iki parmak
    tanındı; sol üst köşe (57,462), sağ alt köşe (710,39): eksenler değişiyor ve köşeler zıt uçlarda,
    SDK'nın panel düzlemindeki dönüşümüyle (test/gt911_touch onunla aynı olduğunu gösteriyor). Home
    iki basışta 14 kez sayıldı: SDK da basışı yeni karede 0x10 bitinin yükselişinden sayar
    (InputManager.cpp pollGt911), yani yonga tuşu defalarca bildirmiş; büyük olasılıkla sağ alt
    köşede tutan parmak alttaki dokunmatik Home tuşuna yakındı. Dokunma sürücüsü cihazda doğrulandı.

## Ekran denetleyicisi (bilgi, risksiz)

X4 Pro'nun 800x480 paneli SSD1677, UC8179 ya da UC8279 denetleyicisiyle gelebilir; C++ firmware
açılışta veriyolunu yoklayıp hangisi olduğunu bulur. Bare-metal ekran sürücüsü önce cihazdakini
hedefleyecek.

- [x] Ayarlar → System → About ekranındaki "Display Controller" satırını bana iletin.
  - Sonuç (2026-10-26, cihaz): UC8179. Bare-metal ekran denemesi UC8179 sürücüsüyle çizecek.

## Bare-metal ekran ("rust: our own UC8279 panel driver, in the bare-metal firmware", "rust: probe the panel controller in the bare-metal firmware", "rust: our own UC8179 panel driver, in the bare-metal firmware", "rust: show the bare-metal touch window on the panel", "rust: the UC8179's grayscale passes", "rust: gray bands in the bare-metal display check")

`purestwhite-epd`, X4 Pro'nun UC8279 ve UC8179 ekran denetleyicilerinin siyah-beyaz sürücüsü ve
UC8179'un 4 seviyeli gri geçişleri; şimdilik yalnızca bare-metal `display-check` imajında. İmaj önce
denetleyiciyi yoklar (C++ firmware'in her açılışta gönderdiği baytların aynısı: 0x71, 0x70,
gerekirse 0xA2) ve UC8279 ya da UC8179 bulursa çizer. UC8179'da ardından dört gri bant gösterir,
CrossPoint'in gri geçişleriyle: iki okuyucu sayfası (yumuşatılmış yazının geçişi, Overlay; ikinci
sayfada bantlar ters sırada ve sayfaya gri sayfadan sonraki yanıp sönmeyen geçişle gelinir), sonra
resim görüntüleyicininki (Absolute), sonra uyku ekranınınki (Direct). SSD1677'de (henüz sürücüsü
yok) başka hiçbir komut göndermez; raporuna hangisi olduğunu yazar. Emülatörde iki denetleyicide de
desen, UC8179'da bantlar da çiziliyor (`emu/baremetal-display-gray.txt`); gri geçişler SDK'nınkilerle
veriyolu düzeyinde aynı (`test/epd_panels`). Bu yüzden deneme her X4 Pro'da yapılabilir (X4
Classic'te değil: orada C++ başka bir yoklama yapar); geri dönüş "Bare-metal ilk deneme"dekiyle
aynıdır, önyükleyici geri alma VAR olmalı. Açılıştan 50 saniye sonra imaj panele yeni bir komut
başlatmaz, sürmekte olanı ilk beklemesinde keser ve kendiliğinden C++'a döner (bekçi 60 saniyede).

- [x] Kartta eski `purestwhite-bare-metal.txt` varsa silin. `purestwhite-baremetal-ekran.bin`'i
  (arşivde, sağlama toplamı `SHA256-bare-metal.txt`'de) koyun; Ayarlar → System → SD Karttan
  Firmware Güncelleme → dosyayı seçin → Onayla. Dokunmadan izleyin. UC8279 ya da UC8179'da ekran
  beyaz zemin, kalın siyah çerçeve, büyük siyah bir blok ve çubuklarla yenilenmeli; 2 saniye sonra
  hızlı yenilemeyle blok kaymalı (yeni yerinde çubukların bir kısmını örter, bu doğru). UC8279'da 3
  saniye sonra C++ ana ekranı gelmeli. UC8179'da 3 saniye sonra dört geniş bant ekranı gelir, her
  biri 3 saniye kalır; bantlar ekranın bir kenarından ötekine siyah, koyu gri, açık gri, beyazdır
  (hangi kenardan başladığı önemli değil):
  1. Okuyucu sayfası: ekran hızlıca üç siyah ve bir beyaz banda geçer, hemen ardından iki siyah
     bant koyu ve açık griye döner.
  2. İkinci okuyucu sayfası: bantlar ters sıraya geçer, ekran yanıp sönmeden.
  3. Resim: ekran yanıp sönüp üç siyah ve bir beyaz banda geçer, sonra iki siyah bant griye döner.
  4. Uyku ekranı geçişi: ekran yenilenip dört bandı doğrudan gösterir.

  Sonra C++ gelmeli. SSD1677'de ekran değişmeden birkaç saniyede C++ gelir. Asıl sınama
  gördüğünüzdür: blok ve çubukların yerini, gölge kalıp kalmadığını, dört bant ekranının her birinde
  dört tonun birbirinden ayrılıp ayrılmadığını (ve hangisinin en iyi göründüğünü), ikinci okuyucu
  sayfasında ilk sayfanın izi kalıp kalmadığını bana anlatın.
- [x] Raporda "panel: ..." satırı denetleyiciyi söyler (About ekranındakiyle aynı olmalı; satırdaki
  "k" yoklamanın kararı, "ver" ve "flg" ham baytlar: uyuşmazsa bunları bana iletin). UC8279 ya da
  UC8179'da "ekran tam: tamam", "ekran hizli: tamam" (UC8179'da sonra "gri: tamam tamam tamam
  tamam": yukarıdaki dört bant ekranı sırayla; "gri tabansiz:" ise imaj okuyucu sayfası için
  belleği bulamamış demektir), "ekran uyku: tamam" ve "bitti", SSD1677'de
  "ekran: atlandi" ve "bitti" bekleniyor. "tamam" yalnızca veriyolu çağrılarının hata vermediğini ve
  denetleyicinin 8 saniyede bitirdiğini gösterir, çizimi değil (veriyolu yalnızca yazar). "hata"
  çoğunlukla BUSY'nin 8 saniyede boşalmadığını (ya da açılıştan 50 saniye geçtiğini) gösterir;
  ondan sonraki adımlar denenmez ve satırları yazılmaz. Ekran donup bir dakika kadar sonra C++ geliyorsa ve bir satır eksikse
  imaj orada takılmış, bekçi sıfırlamıştır. "sonraki acilis:
  SECILEMEDI (...)" yazıp "panel:" satırı yoksa imaj açılışı C++'a veremediği için panele hiç
  dokunmadan sıfırlanmıştır (dönüşü önyükleyicinin geri alması sağlar): bunu bana iletin.
  - Sonuç (2026-10-26, 1e027c71'in imajı, cihaz): "panel: UC8179 k1 ver 000001ffff flg 13" (About
    ekranıyla aynı), "ekran tam: tamam", "ekran hizli: tamam", "gri: tamam tamam tamam tamam",
    "ekran uyku: tamam", "bitti". Sahibine göre hepsi düzgün çizildi: desen ve kayan blok doğru,
    dört bant ekranının her birinde dört ton ayrı görünüyor. Rust ekran sürücüsünün UC8179 siyah-beyaz
    yolu ve dört gri geçişi gerçek panelde doğrulandı. Rapor `firmware-arsiv/cihaz-raporlari/`'nda.

## Bare-metal SD kart ("rust: read the SD card in the bare-metal firmware", "rust: our own FAT file system, read path", "rust: read several SD blocks in one card command")

Bare-metal firmware kartı okuyabiliyor; şimdilik yalnızca okuyor. `sd-check` imajı kartı SDK'nın
yaptığı gibi bağlar: 1 bit SDMMC, 20 MHz, GPIO5 etkinleştirmesinin darbesi, en çok dört deneme. Sonra
FAT bölümünü kendi dosya sistemimizle (`purestwhite-fat`) açar, kökteki öğeleri sayar (ilk
sürümler adlarını da yazıyordu) ve `.crosspoint/onyukleyici.txt`'yi okuyup raporuna yazar. Son
sürümü kendi imaj dosyasını da iki kez okur, önce her blok ayrı komutla, sonra 32 bloğa kadar tek
komutla ve iki süreyi yazar. Karta hiçbir şey yazmaz: dosya sisteminin blok aygıtında yazma yok,
kart sürücüsü de her veri yazmasını reddeder; kitaplarınız ve ayarlarınız olduğu gibi kalır. Kart FAT32 (ya da FAT16) olmalı: exFAT'ı okumuyor, "fat: acilamadi
(Unsupported)" der. Emülatörde kart bağlanıyor, kök listeleniyor, dosya okunuyor
(`emu/baremetal-sd.txt`). Geri dönüş "Bare-metal ilk deneme"dekiyle aynıdır, önyükleyici geri alma
VAR olmalı. Yalnızca X4 Pro'da: X4 Classic'te SD yuvası aynı ama kartın etkinleştirmesi GPIO6,
GPIO5 ise bir düğme.

İlk sınama dosya sistemi henüz `embedded-sdmmc` iken yapıldı, ikincisi `purestwhite-fat`'li
imajla; ikisi de geçti (aşağıda).

`purestwhite-fat` artık yazabiliyor da: dosya oluşturma, yazma ve silme ("rust: write files on
our own FAT file system"); dizin oluşturma ve silme, yeniden adlandırma, kısaltma ("rust:
directories, renames and truncation on our own FAT file system"). Ama `sd-check` bunu kullanmaz,
kart sürücüsü de yazmaları reddetmeye devam eder. Yazma şimdilik yalnızca ana makinede,
imajlarla sınanıyor. Cihaz denemesi ayrı, kitapsız bir test kartında yapılacak; o imaj,
kitaplarınızın olduğu kartı tanıyıp ona yazmayı reddedecek biçimde yapılacak.

- [x] Kartta eski `purestwhite-bare-metal.txt` varsa silin. `purestwhite-baremetal-sd.bin`'i
  (arşivde, sağlama toplamı `SHA256-bare-metal.txt`'de) koyun; Ayarlar → System → SD Karttan
  Firmware Güncelleme → dosyayı seçin → Onayla. Ekran değişmez; birkaç saniye sonra C++ ana ekranı
  gelmeli (kart yanıt vermezse en çok bir dakika). Dokunmadan bekleyin.
- [x] Raporda "sd: deneme 1, tamam, M MB" (M kartın boyutu, 16 GB'lık kartta 15 000 civarı; "deneme 1
  2" gibi birden çok sayı, kartın ancak sonraki denemede bağlandığını söyler), "fat: FAT32, kume N
  B" (N kümenin bayt sayısı; 16 GB'lık kartta çoğunlukla 32768), "kok: K oge: ..."
  (kökteki öğe sayısı ve ilk adlar), "okundu: onyukleyici geri alma: VAR (yuva app1, durum 2)" ve
  "bitti" bekleniyor. "bitti" yoksa ya da başka bir satır çıkarsa ("baglanamadi", "fat: ...",
  "okunamadi", "PANIK" …) raporun tamamını bana iletin; satır bir sayıda yarım kaldıysa ve C++ bir
  dakika kadar sonra geldiyse o denemede kart yanıt vermemiş, bekçi sıfırlamıştır. Kartın
  değişmediğini, kitaplarınızın ve ayarlarınızın yerinde olduğunu C++'ta bir göz atarak doğrulayın.
  - Sonuç (2026-10-26, 04e8dc8e'nin `embedded-sdmmc`'li imajı, cihaz): "sd: deneme 1, tamam, 14910
    MB", "kok: 21 oge: System Volume Information, XTCache, GO", "okundu: onyukleyici geri alma: VAR
    (yuva app1, durum 2)", "bitti". Kart ilk denemede bağlandı; kökteki öğe sayısı bilgisayardaki
    sayımla (rapor dosyası dışında) aynı. Kart sürücüsü ve SDK'nın bağlama sırası gerçek kartta
    doğrulandı. Rapor `firmware-arsiv/cihaz-raporlari/`'nda.
  - Sonuç (2026-10-26, a5340013'ün `purestwhite-fat`'li imajı, cihaz): "sd: deneme 1, tamam, 14910
    MB", "fat: FAT32, kume 8192 B", "kok: 21 oge: System Volume Information, XTCache, GO", "okundu:
    onyukleyici geri alma: VAR (yuva app1, durum 2)", "bitti". Küme boyutu ve kökteki öğe sayısı
    bilgisayardakiyle (`stat -f`, rapor dosyası dışında 21) aynı. Kendi FAT okuma yolumuz gerçek
    kartta doğrulandı. Rapor `firmware-arsiv/cihaz-raporlari/`'nda.
- [x] Çok bloklu okuma: aynı adımlar, yeni `purestwhite-baremetal-sd.bin` ile. Dosyayı kartın
  köküne bu adla koyun: imaj kendini orada bu adla arar. Raporda şunlar bekleniyor:
  - "sd: deneme 1, tamam, M MB, cmd23 var" ya da "... cmd23 yok": kart blok sayısını önceden
    bildiren CMD23'ü alıyor mu; ikisi de olur, hangi yolun denendiğini söyler.
  - "kok: K oge" (artık adlar yazılmıyor) ve "okundu: onyukleyici geri alma: VAR" (dosyanın ilk 26
    baytı).
  - "imaj: N B, tek T1 ms, cok T2 ms, fnv H" ve "bitti". N imaj dosyasının boyutu; H onun sağlaması,
    bilgisayardaki dosyanınkiyle aynı olmalı (ben karşılaştırırım); T2, T1'den belirgin kısa olmalı.

  "imaj: FARKLI ..." iki okumanın farklı veri getirdiğini söyler. "imaj: tek okunamadi (...)" blok
  blok okumada, "imaj: cok okunamadi (...)" çok bloklu okumada bir kart hatası demektir. "imaj: yok"
  dosyanın kökte bu adla bulunmadığını, "imaj: bulunamadi (...)" ya da "imaj: acilamadi (...)" onu
  ararken ya da açarken bir hata çıktığını söyler. Bunlardan biri çıkarsa raporun tamamını bana
  iletin.
  - Sonuç (2026-10-26, 4b2560bf'nin imajı, cihaz): "sd: deneme 1, tamam, 14910 MB, cmd23 var",
    "fat: FAT32, kume 8192 B", "kok: 21 oge", "okundu: onyukleyici geri alma: VAR", "imaj: 131536 B,
    tek 131 ms, cok 72 ms, fnv 90991353", "bitti". Sağlama bilgisayardaki dosyanınkiyle aynı; kart
    CMD23'ü alıyor, yani geri tutulan CMD23 yolu denendi. Çok bloklu okuma blok blok okumadan 1,8
    kat hızlı (yaklaşık 1,8 MB/s; tek veri hattında 20 MHz'in sınırı 2,5 MB/s). Rapor
    `firmware-arsiv/cihaz-raporlari/`'nda.

## Bare-metal düğmeler ("rust: buttons and deep sleep in the bare-metal firmware", "rust: count the buttons without sleeping")

Bare-metal firmware düğmeleri okuyor. `button-check` imajı 40 saniye boyunca Sol, Sağ ve Güç
basmalarını sayar (Sol ve Sağ, SDK'nın Yukarı ve Aşağı dediği tuşlar); Güç'ü bir saniye basılı
tutarsanız saymayı bitirir (hiç uzun basmazsanız 40 saniye dolunca biter, raporda "guc uzun: yok,
40 s doldu" yazar). Güç bırakılınca raporu yazar ve
C++ firmware'e döner. İmaj ekrana hiçbir şey çizmez: ekran güncelleme sayfasında kalır, bu
normaldir. Karta yazmaz. Geri dönüş "Bare-metal ilk deneme"dekiyle aynıdır, önyükleyici geri alma
VAR olmalı. Emülatörde basmalar sayılıyor (`emu/baremetal-buttons.txt`).

Dikkat: Sol tuş (GPIO0) aynı zamanda önyükleme ayağıdır. Cihaz açılırken Sol'u basılı tutmayın;
tutarsanız indirme kipine girer.

İlk sürüm sayımdan sonra derin uykuya giriyordu. Cihazda (2026-10-26) uykudan ne Güç düğmesi ne
60 saniyelik zamanlayıcı uyandırdı; ekran güncelleme sayfasında kaldı, sahibi Güç'e uzun basıp
cihazı yeniden başlattı ve C++ geldi. Tam yeniden başlatma RTC belleğini sildiği için rapor
kayboldu. Emülatörde iki uyanış da çalışıyordu: emülatör derin uykuyu donanım ayrıntısıyla
taklit etmiyor. Uyku, kendini kurtaran ayrı bir tanı imajıyla (`sleep-check`) yeniden denenecek.
Bu denemelerde imaj takılırsa kurtarma yolu Güç'e uzun basmaktır.

- [x] Kartta eski `purestwhite-bare-metal.txt` varsa silin. `purestwhite-baremetal-dugme.bin`'i
  (arşivde, sağlama toplamı `SHA256-bare-metal.txt`'de) koyun; Ayarlar → System → SD Karttan
  Firmware Güncelleme → dosyayı seçin → Onayla. Güncelleme bitince ekran donar (normal). 5 saniye
  bekleyin, sonra 40 saniye içinde aralarında yarım saniye bırakarak Sol'a 3 kez, Sağ'a 2 kez
  kısaca basın, ardından Güç'ü 2 saniye basılı tutup bırakın. Birkaç saniye içinde C++ ana
  ekranı gelmeli.
- [x] Raporda "dugme: sol 3, sag 2, guc 1", "guc uzun: evet" ve "bitti" bekleniyor. Sayılar
  bastıklarınızdan farklıysa raporun tamamını bana iletin. C++ bir dakika içinde gelmezse Güç'e
  uzun basıp yeniden başlatın ve bana söyleyin.
  - Sonuç (2026-10-27, 8a55083e'nin uyuyan imajıyla, cihaz): "dugme: yukari 3, asagi 2, guc 3",
    "guc uzun: evet", "uyku: guc dugmesi ya da 60 s". Sahibi Sol'a 3, Sağ'a 2 kez bastı: sayım
    doğru, düğmeler ve titreşim süzme cihazda doğrulandı (Güç'e birkaç kez basılmış). Sayım kodu
    yeni imajla aynı; yalnızca adlar sol/sağ oldu ve uyku çıktı. Bu kez rapor silinmeden C++'a
    dönüldü; cihazda uyanma nedenini yazan C++ yoktu, nasıl döndüğü belirsiz. Rapor
    `firmware-arsiv/cihaz-raporlari/`'nda.

## Bare-metal uyku tanısı ("rust: a sleep diagnostic that recovers by itself")

İlk derin uyku denemesi cihazda uyanmadı ("Bare-metal düğmeler"e bakın). `sleep-check` imajı uykuyu
adım adım dener ve her adımı rapora yazar:
1. 3 saniyelik hafif uyku, zamanlayıcıyla;
2. Güç'e basılana kadar (en çok 20 saniye) hafif uyku, derin uykunun da kullandığı yoldan;
3. Güç'e basılana kadar ya da 15 saniye derin uyku; uyanış C++ firmware'i açar.

Hafif uykudan sonra imaj kaldığı yerden devam eder ve ne olduğunu kendisi yazar. Derin uykudan
sonra C++ açılır ve raporun sonuna uyanma ve sıfırlama nedenini ekler. Bekçi uykuda da sayar: bir
adım takılırsa 60 saniyede, çip hâlâ çalışıyorsa rapora "uyku: girilmedi" yazar, 15 saniye sonra
çekirdeği sıfırlar (güç mandalı ve rapor korunur, C++ açılır). Bu da olmazsa 30 saniye sonra bütün
sistemi sıfırlar; bu son çare mandalı bırakır, cihaz pildeyse kapanır ve rapor gider. Bekçinin
cihazda uykudan kurtardığı henüz görülmedi. İmaj ekrana çizmez (ekran güncelleme sayfasında
kalır), karta yazmaz. Emülatörde üç adım da çalışıyor (`emu/baremetal-sleep.txt`).

- [x] Önce yeni C++ firmware'i (`purestWhite-next-uyanma.bin`) her zamanki gibi SD ile kurun ve C++
  ana ekranının geldiğini görün. Cihazdaki C++'tan tek farkı, uyku raporlarının sonuna uyanma ve
  sıfırlama nedenini yazması; bu deneme onsuz pek bir şey söylemez.
- [x] Kartta eski `purestwhite-bare-metal.txt` varsa silin. Ayarlar → System → SD Karttan Firmware
  Güncelleme → `purestwhite-baremetal-uyku.bin` → Onayla. Güncelleme bitince ekran donar (normal).
  Sonra:
  1. 10 saniye bekleyin, Güç'e bir kez kısaca basın (hafif uykudan uyandırır).
  2. 5 saniye bekleyin, Güç'ü 2 saniye basılı tutup bırakın (derin uykudan uyandırır).
  3. Hiçbir şeye dokunmayın. Birkaç saniye içinde C++ ana ekranı gelmeli.
- [ ] 2 dakika içinde C++ gelmezse Güç'e kısa basın; olmazsa uzun basıp açın. Hangisinin açtığını
  ve ne kadar beklediğinizi bana söyleyin; rapor çıkmayabilir, bu da bir bulgudur (rapor hiç
  yoksa güç tamamen kesilmiştir).
- [x] Raporda "uyku: hafif 3 s", "hafif 3 s: zamanlayici, ~3000 ms", "uyku: hafif guc", "hafif guc:
  dugme, ...", "uyku: derin, guc ya da 15 s", "uyanma: dugme" ve "sifirlama: DEEPSLEEP" bekleniyor.
  "uyanma: zamanlayici" çıkarsa Güç derin uykudan uyandırmamış, 15 saniyelik zamanlayıcı
  uyandırmıştır. Rapor bir "uyku: ..." satırında bitip ardından "sifirlama: WDT (other)" geliyorsa o
  adım takılmış, bekçi kurtarmıştır; "uyku: girilmedi" de varsa çip uykuya hiç girememiştir. Her
  raporu tamamen bana iletin.
  - Sonuç (2026-10-27, 0249e415'in imajı ve C++'ı, cihaz): "uyku: hafif 3 s", "hafif 3 s:
    zamanlayici, 3001 ms", "uyku: hafif guc", "hafif guc: dugme, 14875 ms", "uyku: derin, guc ya da
    15 s", "uyanma: dugme", "sifirlama: DEEPSLEEP". Üç adım da cihazda çalıştı: hafif uyku
    zamanlayıcıyla ve Güç ile, derin uyku Güç ile (ext1) uyandı; bekçiye gerek kalmadı ("uyku:
    girilmedi" yok). Uyku kodu ilk denemedekiyle aynı; fark bekçide: ilk imajın tek aşamalı sistem
    sıfırlaması güç mandalını bırakıyordu, cihazın kapanıp raporun kaybolması büyük olasılıkla
    bundandı. Rapor `firmware-arsiv/cihaz-raporlari/`'nda.

## Bare-metal PSRAM ("rust: the PSRAM in the bare-metal firmware")

Bare-metal firmware 8 MB'lık PSRAM'i açabiliyor. `psram-check` imajı PSRAM'i sekiz hatlı ve 40 MHz'de
başlatır (C++ 80 MHz kullanır, ama bunun için her açılışta bir zamanlama ayarı yapar; esp-hal bu
ayarı henüz yapmıyor, 40 MHz'de gerekmiyor), yığına ekler, neredeyse tamamını tek bir ayırmayla
alır ve önce bir desenle, sonra tersiyle doldurup geri okur; hataları ve süreleri yazar, sonra C++
firmware'e döner. Uyumaz, karta yazmaz, ekrana çizmez (ekran güncelleme sayfasında kalır, bu
normaldir). Geri dönüş "Bare-metal ilk deneme"dekiyle aynıdır. Emülatörde 8192 KB bulunuyor ve iki
geçiş de hatasız (`emu/baremetal-psram.txt`).

- [ ] Kartta eski `purestwhite-bare-metal.txt` varsa silin. `purestwhite-baremetal-psram.bin`'i
  (arşivde, sağlama toplamı `SHA256-bare-metal.txt`'de) koyun; Ayarlar → System → SD Karttan
  Firmware Güncelleme → dosyayı seçin → Onayla. Ekran donar; hiçbir şeye dokunmayın.
  Birkaç saniye içinde C++ ana ekranı gelmeli. Bir dakika içinde gelmezse Güç'e uzun basıp açın ve
  bana söyleyin.
- [ ] Raporda "psram: 8192 KB, 40 MHz, ...", "yigin: 8128 KB, psram'de", "bellek: hata 0, ..." ve "bellek
  ters: hata 0, ..." ile "bitti" bekleniyor. "hata" sıfırdan büyükse ya da "psram: bulunamadi",
  "yigin: ... ayrilamadi", "PSRAM DISINDA" ya da "PANIK" çıkarsa raporun tamamını bana iletin.

