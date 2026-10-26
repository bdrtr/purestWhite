// Writes checked by others: after each test's writes `fsck.fat -n` must find nothing to mend (it
// also checks that the FATs agree; FSInfo's free count, marked unknown, it only notes), and
// mtools must read back by long name what was written. The tools are required as in `tests`.
#![allow(
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::manual_assert_eq
)]

extern crate std;

use crate::short::{basis, valid_long};

/// The first window of numeric tails: 1 to 1023.
const TAIL_WINDOW_TEST: u32 = 1024;
use crate::tests::{Host, Image, block_on, content, listed, read_all, tools};
use crate::*;
use std::collections::BTreeMap;
use std::format;
use std::string::{String, ToString};
use std::vec;
use std::vec::Vec;

/// Creates `path`, writes `bytes` into it `chunk` bytes at a time, and closes it.
fn write_file<D: BlockWrite>(vol: &mut Volume<D>, path: &str, bytes: &[u8], chunk: usize) {
    let mut file = block_on(vol.create(path)).unwrap();
    for part in bytes.chunks(chunk.max(1)) {
        assert_eq!(block_on(vol.write(&mut file, part)).unwrap(), part.len());
    }
    assert_eq!(file.size() as usize, bytes.len());
    block_on(vol.close(file)).unwrap();
}

/// The volume clean by `fsck.fat`, and every file in `files` read back whole by us and by mtools.
fn check(host: &Host, image: &mut Image, files: &BTreeMap<String, Vec<u8>>) {
    host.store(image);
    let (clean, said) = host.fsck();
    assert!(clean, "fsck.fat:\n{said}");
    let mut vol = block_on(Volume::mount(&mut *image)).unwrap();
    for (path, bytes) in files {
        assert!(read_all(&mut vol, path, 4096) == *bytes, "{path} by us");
        assert!(host.mtype(path) == *bytes, "{path} by mtools");
    }
}

#[test]
fn files_written_are_read_back_by_fsck_and_mtools() {
    if !tools() {
        return;
    }
    for (bits, kib, cluster, mbr) in [
        (16, 32_768, 1, false),
        (32, 40_960, 1, true),
        (32, 40_960, 8, false),
    ] {
        let mut host = Host::new(bits, kib, cluster, mbr);
        host.mkdir("Kitaplar");
        let mut image = host.image();
        let cluster_bytes = usize::from(cluster) * BLOCK;
        let mut files = BTreeMap::new();
        {
            let mut vol = block_on(Volume::mount(&mut image)).unwrap();
            vol.set_time(Timestamp {
                year: 2026,
                month: 12,
                day: 31,
                hour: 23,
                minute: 59,
                second: 59,
            });
            for (i, (path, len, chunk)) in [
                ("README.TXT", 100, 100),
                ("notlar.txt", 511, 7),
                ("Uzun bir kitap adı — Cilt 1.epub", 70_000, 4096),
                ("kısa.txt", 512, 512),
                ("a.b.c.d", 513, 1000),
                ("+artı.txt", cluster_bytes - 1, 333),
                (".gizli", cluster_bytes, cluster_bytes),
                ("boş dosya.bin", 0, 1),
                (
                    "Kitaplar/İçindekiler ve özet.md",
                    3 * cluster_bytes + 1,
                    1500,
                ),
                ("Kitaplar/b.txt", cluster_bytes + 1, 70_000),
            ]
            .into_iter()
            .enumerate()
            {
                let bytes = content(100 + i as u32, len);
                write_file(&mut vol, path, &bytes, chunk);
                files.insert(path.to_string(), bytes);
            }
            // The entry carries the time set.
            let entry = block_on(vol.find("notlar.txt")).unwrap();
            let raw = block_on(vol.read_slot(entry.parent, entry.index)).unwrap();
            let date = ((2026 - 1980) << 9) | (12 << 5) | 31;
            // 59 seconds is kept as 58, in two-second steps.
            let time = (23 << 11) | (59 << 5) | 0x1D;
            assert_eq!(u16::from_le_bytes([raw[24], raw[25]]), date);
            assert_eq!(u16::from_le_bytes([raw[22], raw[23]]), time);
            assert_eq!(raw[11], 0x20);
            // An 8.3 name in upper case is only a short name; the others keep their long one.
            assert_eq!(entry.short_name, *b"NOTLAR  TXT");
            assert_eq!(entry.first + 1, entry.index);
            let readme = block_on(vol.find("README.TXT")).unwrap();
            assert_eq!(readme.first, readme.index);
            assert_eq!(listed(&mut vol, "Kitaplar").len(), 2);
        }
        check(&host, &mut image, &files);
    }
}

#[test]
fn overwrites_and_appends_land_where_they_should() {
    if !tools() {
        return;
    }
    let host = Host::new(32, 40_960, 1, false);
    let mut image = host.image();
    let mut expected = content(1, 10_000);
    {
        let mut vol = block_on(Volume::mount(&mut image)).unwrap();
        let mut file = block_on(vol.create("değişen.bin")).unwrap();
        block_on(vol.write(&mut file, &expected)).unwrap();
        // Over the middle, across block and cluster edges.
        let middle = content(2, 2000);
        file.seek(3000);
        block_on(vol.write(&mut file, &middle)).unwrap();
        expected[3000..5000].copy_from_slice(&middle);
        // Over the end and past it.
        let tail = content(3, 3000);
        file.seek(9000);
        block_on(vol.write(&mut file, &tail)).unwrap();
        expected.truncate(9000);
        expected.extend_from_slice(&tail);
        // A seek past the end stops at it.
        file.seek(1_000_000);
        assert_eq!(file.position(), 12_000);
        let more = content(4, 5000);
        block_on(vol.write(&mut file, &more)).unwrap();
        expected.extend_from_slice(&more);
        // The entry learns the size when the file is synced.
        assert_eq!(block_on(vol.find("değişen.bin")).unwrap().size(), 0);
        block_on(vol.sync(&mut file)).unwrap();
        let entry = block_on(vol.find("değişen.bin")).unwrap();
        assert_eq!(entry.size() as usize, expected.len());
        block_on(vol.close(file)).unwrap();
    }
    let files = BTreeMap::from([(String::from("değişen.bin"), expected)]);
    check(&host, &mut image, &files);
}

#[test]
fn short_names_get_tails_that_do_not_clash() {
    if !tools() {
        return;
    }
    let host = Host::new(32, 40_960, 1, false);
    let mut image = host.image();
    let mut files = BTreeMap::new();
    {
        let mut vol = block_on(Volume::mount(&mut image)).unwrap();
        for i in 1..=30 {
            let path = format!("kitap bölüm {i:02}.txt");
            let bytes = content(i, 10);
            write_file(&mut vol, &path, &bytes, 10);
            files.insert(path, bytes);
        }
        let short = |vol: &mut Volume<&mut Image>, i: u32| {
            block_on(vol.find(&format!("kitap bölüm {i:02}.txt")))
                .unwrap()
                .short_name
        };
        assert_eq!(&short(&mut vol, 1), b"KITAPB~1TXT");
        assert_eq!(&short(&mut vol, 9), b"KITAPB~9TXT");
        assert_eq!(&short(&mut vol, 10), b"KITAP~10TXT");
        // A freed tail is taken again; a name whose basis is free keeps it.
        block_on(vol.remove("kitap bölüm 05.txt")).unwrap();
        files.remove("kitap bölüm 05.txt");
        write_file(&mut vol, "kitap bölüm 99.txt", b"x", 1);
        files.insert(String::from("kitap bölüm 99.txt"), b"x".to_vec());
        assert_eq!(&short(&mut vol, 99), b"KITAPB~5TXT");
        write_file(&mut vol, "kitapbl.txt", b"y", 1);
        files.insert(String::from("kitapbl.txt"), b"y".to_vec());
        assert_eq!(
            &block_on(vol.find("kitapbl.txt")).unwrap().short_name,
            b"KITAPBL TXT"
        );
    }
    check(&host, &mut image, &files);
}

#[test]
fn names_that_clash_or_cannot_be_are_refused() {
    if !tools() {
        return;
    }
    let mut host = Host::new(16, 32_768, 1, false);
    host.put("README.TXT", content(1, 10));
    host.put("Uzun ad.txt", content(2, 10));
    let mut image = host.image();
    {
        let mut vol = block_on(Volume::mount(&mut image)).unwrap();
        for taken in [
            "readme.txt",
            "UZUN AD.TXT",
            "uzunad~1.txt",
            "  Uzun ad.txt. ",
        ] {
            assert!(
                matches!(block_on(vol.create(taken)), Err(Error::Exists)),
                "{taken}"
            );
        }
        let long: String = "a".repeat(256);
        for bad in ["", ".", "..", "a:b", "soru?", "a\u{1}b", long.as_str()] {
            assert!(
                matches!(block_on(vol.create(bad)), Err(Error::BadName)),
                "{bad:?}"
            );
        }
        assert!(matches!(
            block_on(vol.create("yok/dosya.txt")),
            Err(Error::NotFound)
        ));
        assert!(matches!(
            block_on(vol.create("README.TXT/dosya.txt")),
            Err(Error::NotADirectory)
        ));
        // 255 units is the most, and fine.
        let longest: String = "b".repeat(255);
        block_on(vol.create(&longest)).unwrap();
    }
    host.store(&image);
    let (clean, said) = host.fsck();
    assert!(clean, "fsck.fat:\n{said}");
}

#[test]
fn a_directory_grows_by_clusters() {
    if !tools() {
        return;
    }
    let mut host = Host::new(32, 40_960, 1, false);
    host.mkdir("çok");
    // Free clusters that are not zero, as on a used card: a directory's new cluster must be
    // zeroed, or what was there reads as entries.
    host.put("çöp.bin", vec![0xA5; 4 * 1024 * 1024]);
    host.delete("çöp.bin");
    let mut image = host.image();
    let mut files = BTreeMap::new();
    {
        let mut vol = block_on(Volume::mount(&mut image)).unwrap();
        // Three entries each, sixteen to a cluster: about forty clusters.
        for i in 0..200 {
            let path = format!("çok/dosya numarası {i:03}.txt");
            let bytes = content(i, (i % 3) as usize * 300);
            write_file(&mut vol, &path, &bytes, 1000);
            files.insert(path, bytes);
        }
        assert_eq!(listed(&mut vol, "çok").len(), 200);
    }
    check(&host, &mut image, &files);
}

#[test]
fn fat16_s_root_fills_up() {
    if !tools() {
        return;
    }
    let host = Host::new(16, 32_768, 1, false);
    let mut image = host.image();
    let mut made = 0;
    {
        let mut vol = block_on(Volume::mount(&mut image)).unwrap();
        // Two entries each (a long name of lower case); the label takes one of the 512.
        loop {
            match block_on(vol.create(&format!("f{made:03}.txt"))) {
                Ok(_) => made += 1,
                Err(Error::DirFull) => break,
                Err(e) => panic!("{e:?}"),
            }
        }
    }
    assert_eq!(made, 255);
    host.store(&image);
    let (clean, said) = host.fsck();
    assert!(clean, "fsck.fat:\n{said}");
}

#[test]
fn a_full_volume_keeps_what_was_written() {
    if !tools() {
        return;
    }
    // About 8 000 clusters of one block.
    let host = Host::new(16, 4096, 1, false);
    let mut image = host.image();
    let big = content(9, 8 * 1024 * 1024);
    let written;
    {
        let mut vol = block_on(Volume::mount(&mut image)).unwrap();
        let mut file = block_on(vol.create("büyük.bin")).unwrap();
        assert!(matches!(
            block_on(vol.write(&mut file, &big)),
            Err(Error::Full)
        ));
        written = file.size() as usize;
        assert!(written > 3 * 1024 * 1024, "{written}");
        // Full is known now: the next try does not read the FAT again.
        let before = vol.device_mut().calls;
        assert!(matches!(
            block_on(vol.write(&mut file, &big[..1])),
            Err(Error::Full)
        ));
        assert_eq!(vol.device_mut().calls, before);
        block_on(vol.close(file)).unwrap();
        assert_eq!(
            block_on(vol.find("büyük.bin")).unwrap().size() as usize,
            written
        );
    }
    let files = BTreeMap::from([(String::from("büyük.bin"), big[..written].to_vec())]);
    check(&host, &mut image, &files);
    // Still full in a new mount; once the file is removed, its room is there again.
    {
        let mut vol = block_on(Volume::mount(&mut image)).unwrap();
        let mut again = block_on(vol.create("yeniden.bin")).unwrap();
        assert!(matches!(
            block_on(vol.write(&mut again, b"x")),
            Err(Error::Full)
        ));
        block_on(vol.remove("büyük.bin")).unwrap();
        again.seek(0);
        for part in big[..written].chunks(65_536) {
            assert_eq!(block_on(vol.write(&mut again, part)).unwrap(), part.len());
        }
        block_on(vol.close(again)).unwrap();
    }
    let files = BTreeMap::from([(String::from("yeniden.bin"), big[..written].to_vec())]);
    check(&host, &mut image, &files);
}

#[test]
fn removed_files_give_their_clusters_back() {
    if !tools() {
        return;
    }
    let mut host = Host::new(32, 40_960, 1, true);
    host.mkdir("Dizin");
    host.put("Dizin/mtools'un dosyası.bin", content(1, 5000));
    let mut image = host.image();
    let mut files = BTreeMap::new();
    {
        let mut vol = block_on(Volume::mount(&mut image)).unwrap();
        for i in 0..20 {
            let path = format!("silinecek {i}.bin");
            let bytes = content(i, i as usize * 1000);
            write_file(&mut vol, &path, &bytes, 3000);
            files.insert(path, bytes);
        }
        for i in (0..20).step_by(2) {
            let path = format!("silinecek {i}.bin");
            block_on(vol.remove(&path)).unwrap();
            files.remove(&path);
            assert!(matches!(block_on(vol.find(&path)), Err(Error::NotFound)));
        }
        block_on(vol.remove("Dizin/mtools'un dosyası.bin")).unwrap();
        assert!(matches!(
            block_on(vol.remove("yok.bin")),
            Err(Error::NotFound)
        ));
        assert!(matches!(
            block_on(vol.remove("Dizin")),
            Err(Error::NotAFile)
        ));
        assert!(listed(&mut vol, "Dizin").is_empty());
    }
    check(&host, &mut image, &files);
}

#[test]
fn with_mirroring_off_only_the_fat_in_use_changes() {
    if !tools() {
        return;
    }
    let host = Host::new(32, 40_960, 1, false);
    let mut image = host.image();
    let b = &image.bytes;
    let reserved = usize::from(u16::from_le_bytes([b[14], b[15]]));
    let fat = u32::from_le_bytes([b[36], b[37], b[38], b[39]]) as usize * BLOCK;
    let first = reserved * BLOCK;
    // The second FAT in use, mirroring off.
    image.bytes[40] = 0x81;
    let before = image.bytes[first..first + fat].to_vec();
    {
        let mut vol = block_on(Volume::mount(&mut image)).unwrap();
        write_file(&mut vol, "ikinci FAT.bin", &content(1, 5000), 5000);
    }
    assert!(image.bytes[first..first + fat] == before[..]);
    assert!(image.bytes[first + fat..first + 2 * fat] != before[..]);
    let mut vol = block_on(Volume::mount(&mut image)).unwrap();
    assert!(read_all(&mut vol, "ikinci FAT.bin", 512) == content(1, 5000));
}

#[test]
fn basis_names_and_tails() {
    let cases = [
        ("README.TXT", b"README  TXT", true, false),
        ("notlar.txt", b"NOTLAR  TXT", false, false),
        ("kısa.txt", b"K_SA    TXT", false, true),
        ("a.b.c.d", b"ABC     D  ", false, true),
        (".gizli", b"GIZLI      ", false, true),
        ("çok uzun bir ad", b"_OKUZUNB   ", false, true),
        ("+.txt", b"_       TXT", false, true),
        ("kitap.epub", b"KITAP   EPU", false, true),
        ("A B.TXT", b"AB      TXT", false, true),
    ];
    for (long, short, exact, tail) in cases {
        let b = basis(long);
        assert_eq!(&b.name, short, "{long}");
        assert_eq!((b.exact, b.tail), (exact, tail), "{long}");
    }
    let b = basis("Uzun bir kitap adı.epub");
    assert_eq!(&b.with_tail(1), b"UZUNBI~1EPU");
    assert_eq!(&b.with_tail(42), b"UZUNB~42EPU");
    assert_eq!(&b.with_tail(999_999), b"U~999999EPU");
    for n in [1, 42, 999_999] {
        assert_eq!(b.tail_of(&b.with_tail(n)), Some(n));
    }
    // Another extension, a leading zero, a tail not where it would be: not this basis'.
    assert_eq!(b.tail_of(b"UZUNBI~1TXT"), None);
    assert_eq!(b.tail_of(b"UZUNB~01EPU"), None);
    assert_eq!(b.tail_of(b"UZUN~1  EPU"), None);
    // A short basis keeps all of itself.
    assert_eq!(&basis("ab.txt").with_tail(3), b"AB~3    TXT");
    for (name, ok) in [
        ("a", true),
        ("çok güzel kitap", true),
        ("", false),
        ("..", false),
        ("a|b", false),
        ("a\"b", false),
    ] {
        assert_eq!(valid_long(name), ok, "{name:?}");
    }
}

#[test]
fn fat32_clusters_past_65535_go_into_both_halves() {
    if !tools() {
        return;
    }
    let mut host = Host::new(32, 40_960, 1, false);
    host.put("dolgu.bin", content(3, 36 * 1024 * 1024));
    let mut image = host.image();
    let bytes = content(4, 20_000);
    {
        let mut vol = block_on(Volume::mount(&mut image)).unwrap();
        write_file(&mut vol, "yüksek küme.bin", &bytes, 20_000);
        let entry = block_on(vol.find("yüksek küme.bin")).unwrap();
        assert!(entry.cluster > 0xFFFF, "cluster {}", entry.cluster);
    }
    let files = BTreeMap::from([(String::from("yüksek küme.bin"), bytes)]);
    check(&host, &mut image, &files);
}

#[test]
fn whole_blocks_written_past_the_cache_are_not_read_stale() {
    let mut image = Image {
        bytes: vec![0; 64 * BLOCK],
        calls: 0,
    };
    // Not a volume: the cache and the device alone.
    let mut vol = Volume {
        dev: &mut image,
        layout: Layout {
            kind: Kind::Fat16,
            fat_start: 1,
            fat_blocks: 1,
            fats_start: 1,
            fats: 1,
            mirrored: true,
            fsinfo: None,
            root_start: 2,
            root_entries: 16,
            root_cluster: 0,
            data_start: 3,
            cluster_blocks: 1,
            cluster_count: 60,
        },
        fat: Cached::new(),
        data: Cached::new(),
        fat_w: Cached::new(),
        fat_dirty: false,
        next_free: 2,
        full: false,
        fsinfo_marked: false,
        removals: 0,
        stamp: Timestamp::default(),
    };
    block_on(vol.write_part(5, 10, b"eski")).unwrap();
    block_on(vol.write_blocks(4, &[[1; BLOCK], [2; BLOCK]])).unwrap();
    assert_eq!(block_on(vol.data_block(5)).unwrap()[10], 2);
}

/// A device that stops writing after `left` writes.
struct Cut<'a> {
    image: &'a mut Image,
    left: usize,
    writes: usize,
}

impl BlockDevice for Cut<'_> {
    type Error = ();

    async fn read(&mut self, start: u32, blocks: &mut [Block]) -> Result<(), ()> {
        self.image.read(start, blocks).await
    }
}

impl BlockWrite for Cut<'_> {
    async fn write(&mut self, start: u32, blocks: &[Block]) -> Result<(), ()> {
        if self.left == 0 {
            return Err(());
        }
        self.left -= 1;
        self.writes += 1;
        self.image.write(start, blocks).await
    }
}

/// Some writes: a file created and written in pieces (its chain grows), another removed, a
/// directory grown by a cluster, the first file grown again and closed.
fn some_writes(dev: Cut<'_>) -> Result<usize, Error<()>> {
    let mut vol = block_on(Volume::mount(dev))?;
    let mut file = block_on(vol.create("kesilen yazma.bin"))?;
    block_on(vol.write(&mut file, &content(1, 1500)))?;
    block_on(vol.write(&mut file, &content(2, 1500)))?;
    block_on(vol.sync(&mut file))?;
    block_on(vol.remove("silinecek dosya.bin"))?;
    for i in 0..6 {
        let mut other = block_on(vol.create(&format!("Dizin/uzunca bir ad {i}.txt")))?;
        block_on(vol.write(&mut other, b"x"))?;
        block_on(vol.close(other))?;
    }
    block_on(vol.write(&mut file, &content(3, 2000)))?;
    block_on(vol.close(file))?;
    Ok(vol.into_inner().writes)
}

#[test]
fn a_cut_at_any_write_leaves_nothing_worse_than_lost_clusters() {
    if !tools() {
        return;
    }
    let mut host = Host::new(32, 40_960, 1, false);
    host.mkdir("Dizin");
    host.put("silinecek dosya.bin", content(9, 3000));
    let base = host.image();
    let mut whole = Image {
        bytes: base.bytes.clone(),
        calls: 0,
    };
    let all = some_writes(Cut {
        image: &mut whole,
        left: usize::MAX,
        writes: 0,
    })
    .unwrap();
    assert!(all > 20, "{all} writes");
    // What a cut may leave, which fsck mends: lost clusters, a file shorter than its chain, a
    // long name without its entry, FAT copies written in part.
    for left in 0..all {
        let mut image = Image {
            bytes: base.bytes.clone(),
            calls: 0,
        };
        let _ = some_writes(Cut {
            image: &mut image,
            left,
            writes: 0,
        });
        host.store(&image);
        let (_, said) = host.fsck();
        for bad in [
            "free cluster",
            "shared",
            "cross",
            "Duplicate",
            "bad cluster",
        ] {
            assert!(!said.contains(bad), "cut after {left} writes:\n{said}");
        }
    }
}

#[test]
fn a_file_removed_while_open_does_not_write_into_another_entry() {
    if !tools() {
        return;
    }
    let host = Host::new(32, 40_960, 1, false);
    let mut image = host.image();
    {
        let mut vol = block_on(Volume::mount(&mut image)).unwrap();
        let mut old = block_on(vol.create("eski.bin")).unwrap();
        block_on(vol.write(&mut old, &content(1, 3000))).unwrap();
        block_on(vol.sync(&mut old)).unwrap();
        block_on(vol.remove("eski.bin")).unwrap();
        // The new file takes the old one's entries.
        write_file(&mut vol, "yeni.bin", &content(2, 100), 100);
        let new = block_on(vol.find("yeni.bin")).unwrap();
        assert_eq!(new.index, old.index);
        // Its next write finds its entry gone, before writing anything.
        let calls = vol.device_mut().calls;
        assert!(matches!(
            block_on(vol.write(&mut old, b"daha")),
            Err(Error::NotFound)
        ));
        assert_eq!(vol.device_mut().calls, calls);
        assert!(read_all(&mut vol, "yeni.bin", 512) == content(2, 100));
    }
    let files = BTreeMap::from([(String::from("yeni.bin"), content(2, 100))]);
    // The old file's clusters were freed, and nothing more was written through it.
    check(&host, &mut image, &files);
}

#[test]
fn fsinfo_is_marked_unknown_once() {
    if !tools() {
        return;
    }
    let host = Host::new(32, 40_960, 1, false);
    let mut image = host.image();
    let at = usize::from(u16::from_le_bytes([image.bytes[48], image.bytes[49]])) * BLOCK;
    assert_ne!(&image.bytes[at + 488..at + 492], &[0xFF; 4]);
    {
        let mut vol = block_on(Volume::mount(&mut image)).unwrap();
        write_file(&mut vol, "bir.bin", &content(1, 5000), 5000);
        let calls = vol.device_mut().calls;
        write_file(&mut vol, "iki.bin", &[], 1);
        // A create without clusters: its entry and nothing else (no FSInfo again).
        assert!(vol.device_mut().calls - calls <= 4);
    }
    assert_eq!(&image.bytes[at + 488..at + 492], &[0xFF; 4]);
    let next = u32::from_le_bytes(image.bytes[at + 492..at + 496].try_into().unwrap());
    assert!(next > 2, "{next}");
    host.store(&image);
    let (clean, said) = host.fsck();
    assert!(clean && said.contains("uninitialized"), "fsck.fat:\n{said}");
}

#[test]
fn tails_past_the_first_window_are_found() {
    if !tools() {
        return;
    }
    let host = Host::new(32, 40_960, 1, false);
    let mut image = host.image();
    {
        let mut vol = block_on(Volume::mount(&mut image)).unwrap();
        // Short names only, as the tails of "kitap bölüm" would be: 1 to 1023 taken.
        let b = basis("kitap bölüm.txt");
        for n in 1..TAIL_WINDOW_TEST {
            let short = b.with_tail(n);
            let base = core::str::from_utf8(&short[..8]).unwrap().trim_end();
            let name = format!("{base}.TXT");
            let file = block_on(vol.create(&name)).unwrap();
            assert_eq!(file.short, short);
        }
        // And the second window's first, so the free one is 1025.
        block_on(vol.create("KIT~1024.TXT")).unwrap();
        let file = block_on(vol.create("kitap bölüm.txt")).unwrap();
        assert_eq!(&file.short, b"KIT~1025TXT");
    }
    host.store(&image);
    let (clean, said) = host.fsck();
    assert!(clean, "fsck.fat:\n{said}");
}

/// A device whose first write to `block` fails.
struct FailOnce<'a> {
    image: &'a mut Image,
    block: u32,
    failed: bool,
}

impl BlockDevice for FailOnce<'_> {
    type Error = ();

    async fn read(&mut self, start: u32, blocks: &mut [Block]) -> Result<(), ()> {
        self.image.read(start, blocks).await
    }
}

impl BlockWrite for FailOnce<'_> {
    async fn write(&mut self, start: u32, blocks: &[Block]) -> Result<(), ()> {
        if start == self.block && !self.failed {
            self.failed = true;
            return Err(());
        }
        self.image.write(start, blocks).await
    }
}

#[test]
fn fsinfo_is_marked_after_a_failed_try() {
    if !tools() {
        return;
    }
    let host = Host::new(32, 40_960, 1, false);
    let mut image = host.image();
    let block = u16::from_le_bytes([image.bytes[48], image.bytes[49]]);
    let at = usize::from(block) * BLOCK;
    {
        let dev = FailOnce {
            image: &mut image,
            block: u32::from(block),
            failed: false,
        };
        let mut vol = block_on(Volume::mount(dev)).unwrap();
        let mut file = block_on(vol.create("bir.bin")).unwrap();
        block_on(vol.write(&mut file, &content(1, 5000))).unwrap();
        assert!(matches!(
            block_on(vol.sync(&mut file)),
            Err(Error::Device(()))
        ));
        // The FAT did not go out without the mark; now both do.
        block_on(vol.close(file)).unwrap();
    }
    assert_eq!(&image.bytes[at + 488..at + 492], &[0xFF; 4]);
    let files = BTreeMap::from([(String::from("bir.bin"), content(1, 5000))]);
    check(&host, &mut image, &files);
}
