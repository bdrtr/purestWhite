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
  `Time to parse and build pages`). Rust çözücüsü host'ta %10–20 yavaş, cihazda ölçülmedi.
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
