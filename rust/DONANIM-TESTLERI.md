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
- [ ] KOReader eşitleme: konum gönderme ve alma.
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
- [ ] Dipnot bağlantıları olan bir kitap: bağlantıya dokunulunca dipnot açılmalı.
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
- [ ] Sözlük (.dict.dz): kelime aramaları tanımları getirmeli.

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

- [ ] Cihazda ayrıca denenecek bir şey yok: "PNG resimleri" bölümündeki `png-broken.epub`
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
- [ ] Aynı klasördeki `png-broken.epub`: bölüm açılmalı, cihaz donmamalı. Günlükte
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
- [ ] Uyku ekranı ayarını **Transparent** yapın, kartın köküne alfa kanallı bir
  `sleep-overlay.png` koyun (kökte `sleep-overlay.bmp` olmasın, o önce gelir), cihazı uyutun:
  saydam yerlerden alttaki ekran, opak yerlerde resim görünmeli.
- [ ] BLE kumanda bağlıyken PNG'li bir bölümü ilk kez açın: kumanda bağlı kalmalı, sayfa
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

- [ ] `python3 emu/make_jpeg_epub.py emu/books/jpeg` ile üretilen `jpeg-broken.epub`'ı karta
  kopyalayıp bölümünü açın: sayfa açılmalı, cihaz donmamalı. Sayfada yalnızca başlık görünür,
  resim atlanır (günlükte `Failed to open JPEG for dimensions (err=2)`); emülatörde de böyle. Eski
  sürüm bu resimde sonsuza dek bekliyordu. Sağlam JPEG'ler bu değişiklikten etkilenmez.

## Hizalı Rust blokları ("fix: aligned Rust allocations follow malloc's PSRAM rule")

- [ ] Görünür bir değişiklik yok: 4 bayttan geniş hizalanan Rust blokları (XML ayrıştırıcıları, OPF
  okuyucu) artık bir `malloc` bloğundan kesiliyor. Önbelleği silinmiş birkaç kitabı açıp bölümler
  arasında gezinin, OPDS kataloğuna da göz atın: günlükte `Rust panic`, yeniden başlatma ya da donma
  olmamalı.
- [ ] BLE kumanda bağlıyken bir kitabı ilk kez açın: kumanda bağlı kalmalı, sayfa çevirme çalışmalı.

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
- [ ] Sözlük (.dict.dz): birkaç kelime, uzun tanımlı bir kelime de arayın; tanımlar tam gelmeli.
- [ ] KOReader eşitleme: konum gönderme ve alma çalışmalı.
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

- [ ] Ayarlar → Manage Fonts / Yazı Tiplerini Yönet'ten indirilmiş ya da `/.fonts` (veya `/fonts`)
  altına konmuş bir fontu Text Settings / Metin ayarları → Font / Yazı tipi'nde okuma fontu olarak
  seçin ve bir kitap açın: metin o fontla, kalın ve italik yerler doğru stille görünmeli.
- [ ] Bu fontla bir kitap açın, kapatın, cihazı yeniden başlatıp kitabı yeniden açın: bölümler
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

- [ ] Karttaki sağlam bir fontla birkaç sayfa okuyun: harfler eksiksiz görünmeli (hiçbir harf boş
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

- [ ] Karttaki bir fontla daha önce bu fontla açılmamış bir kitap açın ya da yazı boyutunu
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

- [ ] Karttaki bir fontu okuma fontu seçip daha önce bu fontla açılmamış bir kitap açın ya da yazı
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

- [ ] Önceki firmware'le açılmış bir kitabı (bir bölümü tam dizilmiş, uzun bir bölümü yarım
  kalmış) yeni sürümü yükledikten sonra açın: yeniden dizilmeden hemen açılmalı, günlükte
  `Unknown version` ya da `Parameters do not match` olmamalı.
- [ ] Bir kitabı açıp birkaç sayfa okuyun, kitaptan çıkıp yeniden açın: aynı sayfada ve hemen
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

- [ ] İç bağlantıları (dipnot, içindekiler bağlantısı) olan bir bölümde bağlantılara dokunun
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
- [ ] Resimli bir kitapta resim içeren sayfalar ve yer tutucuları doğru yerde olmalı.
- [ ] Ayarlar'da Gömülü Stil açıkken, CSS'li bir kitapta (kalın, italik, altı/üstü çizili, üst ve
  alt simge, küçük büyük harf, farklı yazı boyutlu sözcükler; `::first-letter` ya da büyük ilk harf
  ile başlayan paragraflar; kenarlıklı ya da açık arka planlı kutular) sayfalar yeniden açınca aynı
  görünmeli. `<hr>` çizgileri Gömülü Stil kapalıyken de görünmeli.
- [ ] Odaklanmış Okuma açıkken Latin harfli bir kitapta sözcük başları yeniden açınca da kalın
  olmalı.
- [ ] Okuma yazı tipi olarak CJK destekli bir SD fontu seçiliyken furiganalı Japonca bir kitapta
  (ör. bir Aozora Bunko EPUB'u) ruby metinleri yeniden açınca yerinde olmalı.
- [ ] Dipnot ve iç bağlantılı bir bölümde bağlantılar yeniden açınca da doğru yere gitmeli.
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
- [ ] CSS'inde `::first-letter` ya da `initial-letter` olan (ya da büyük ilk harf span'ı kullanan) bir
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

- [ ] CSS'inde `::first-letter` ya da `initial-letter` olan (ya da büyük ilk harf span'ı kullanan) bir
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
öğelerin boyutlarıyla çarpılır (rem olan bir öğe değeri baştan koyar), üstüne bloğun CSS `font-
size`'ı (rem ise doğrudan, değilse ebeveyne göre) ya da başlıkların h1–h6 varsayılan oranları
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

- [ ] Başlıkları (h1–h6), `font-size` verilmiş paragrafları ve yazı boyutu büyütülmüş ya da
  küçültülmüş span'ları (`small`/`big` ya da `font-size` verilmiş) olan bir kitabı açın (`python3
  emu/make_css_epub.py <klasör>` bir kısmını üretir): yazı boyutları eskisi gibi görünmeli.
