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
her istek için yeni bağlantı açar; varsayılan sunucuyla bir eşitleme ≈5–6 s uzayabilir.

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

- [ ] CrossPoint Sync (varsayılan sunucu sync.crosspointreader.com): konum gönderip alın,
  çalışmalı. Seri günlükteki `[SecureClient] handshake ok (...) in N ms` sürelerini ve tüm
  eşitlemenin süresini önceki sürümle karşılaştırın (yukarıdaki ek süre beklenir).
- [ ] OPDS: herkese açık bir katalogda (ör. `https://m.gutenberg.org/ebooks.opds/`) gezinip
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
