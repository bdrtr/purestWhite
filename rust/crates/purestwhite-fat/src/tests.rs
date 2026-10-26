// Volumes made by the host's dosfstools and mtools (mkfs.fat, mmd, mcopy, mdel): what we put in
// them must come back. Without the tools the tests fail, unless PW_FAT_SKIP_TOOLS is set: then
// they say so and pass. The fakes index directly, and a wrong result fails the test.
#![allow(
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    // Byte buffers are compared with assert!, so a failure does not print them whole.
    clippy::manual_assert_eq
)]

extern crate std;

use super::*;
use core::pin::pin;
use core::task::{Context, Poll, Waker};
use std::collections::BTreeMap;
use std::format;
use std::path::PathBuf;
use std::process::Command;
use std::string::{String, ToString};
use std::sync::atomic::{AtomicU32, Ordering};
use std::vec;
use std::vec::Vec;

/// Runs a future of the fakes below, which are always ready.
fn block_on<F: Future>(future: F) -> F::Output {
    match pin!(future)
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => unreachable!("the fakes are always ready"),
    }
}

/// A volume image in memory, counting the device calls.
struct Image {
    bytes: Vec<u8>,
    calls: usize,
}

impl BlockDevice for Image {
    type Error = ();

    #[allow(clippy::unused_async_trait_impl)]
    async fn read(&mut self, start: u32, blocks: &mut [Block]) -> Result<(), ()> {
        self.calls += 1;
        let at = start as usize * BLOCK;
        let src = self.bytes.get(at..at + blocks.len() * BLOCK).ok_or(())?;
        for (to, from) in blocks.iter_mut().zip(src.as_chunks::<BLOCK>().0) {
            to.copy_from_slice(from);
        }
        Ok(())
    }
}

/// Whether dosfstools and mtools are there; without them a test fails, or with
/// `PW_FAT_SKIP_TOOLS` set says so and passes.
fn tools() -> bool {
    let found = Command::new("mkfs.fat").arg("--help").output().is_ok()
        && Command::new("mcopy").arg("-V").output().is_ok();
    if !found {
        assert!(
            std::env::var_os("PW_FAT_SKIP_TOOLS").is_some(),
            "dosfstools and mtools are required (PW_FAT_SKIP_TOOLS=1 skips these tests)"
        );
        std::eprintln!("skipped: dosfstools and mtools are not installed");
    }
    found
}

/// Bytes of a file, from a seed.
fn content(seed: u32, len: usize) -> Vec<u8> {
    let mut x = seed.wrapping_mul(2_654_435_761) | 1;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x as u8
        })
        .collect()
}

static NEXT: AtomicU32 = AtomicU32::new(0);

/// A volume being built on the host, deleted when dropped.
struct Host {
    path: PathBuf,
    /// Where the volume starts in the image, for mtools.
    offset: u64,
    /// What the volume should hold: directories (as `Some` of nothing) and files.
    tree: BTreeMap<String, Option<Vec<u8>>>,
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

impl Host {
    /// An image of `kib` KiB of FAT `bits` with clusters of `cluster` blocks, in an MBR partition
    /// at block 2048 when asked.
    fn new(bits: u8, kib: u64, cluster: u8, mbr: bool) -> Host {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("pw-fat-{}-{n}.img", std::process::id()));
        let offset = if mbr { 2048 * 512 } else { 0 };
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(offset + kib * 1024).unwrap();
        drop(file);
        if mbr {
            let mut image = std::fs::read(&path).unwrap();
            let entry = &mut image[0x1BE..0x1CE];
            entry[4] = if bits == 32 { 0x0C } else { 0x06 };
            entry[8..12].copy_from_slice(&2048u32.to_le_bytes());
            entry[12..16].copy_from_slice(&((kib * 2) as u32).to_le_bytes());
            image[510] = 0x55;
            image[511] = 0xAA;
            std::fs::write(&path, image).unwrap();
        }
        let mut mkfs = Command::new("mkfs.fat");
        mkfs.args([
            "-F",
            &bits.to_string(),
            "-s",
            &cluster.to_string(),
            "-n",
            "PWTEST",
            "-i",
            "1234ABCD",
        ]);
        if mbr {
            mkfs.args(["--offset", "2048"]);
        }
        let out = mkfs.arg(&path).arg(kib.to_string()).output().unwrap();
        assert!(
            out.status.success(),
            "mkfs.fat: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        Host {
            path,
            offset,
            tree: BTreeMap::new(),
        }
    }

    fn mtools(&self, tool: &str, args: &[&str]) {
        let image = format!("{}@@{}", self.path.display(), self.offset);
        let out = Command::new(tool)
            .env("MTOOLS_SKIP_CHECK", "1")
            .env("LC_ALL", "C.UTF-8")
            .args(["-i", &image])
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{tool} {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn mkdir(&mut self, path: &str) {
        self.mtools("mmd", &[&format!("::/{path}")]);
        self.tree.insert(path.to_string(), None);
    }

    fn put(&mut self, path: &str, bytes: Vec<u8>) {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let source = std::env::temp_dir().join(format!("pw-fat-src-{}-{n}", std::process::id()));
        std::fs::write(&source, &bytes).unwrap();
        self.mtools("mcopy", &[source.to_str().unwrap(), &format!("::/{path}")]);
        let _ = std::fs::remove_file(&source);
        self.tree.insert(path.to_string(), Some(bytes));
    }

    fn delete(&mut self, path: &str) {
        self.mtools("mdel", &[&format!("::/{path}")]);
        self.tree.remove(path);
    }

    fn image(&self) -> Image {
        Image {
            bytes: std::fs::read(&self.path).unwrap(),
            calls: 0,
        }
    }

    /// The names directly in `dir` ("" for the root), sorted.
    fn names_in(&self, dir: &str) -> Vec<String> {
        let prefix = if dir.is_empty() {
            String::new()
        } else {
            format!("{dir}/")
        };
        let mut names: Vec<String> = self
            .tree
            .keys()
            .filter_map(|k| k.strip_prefix(&prefix))
            .filter(|rest| !rest.is_empty() && !rest.contains('/'))
            .map(ToString::to_string)
            .collect();
        names.sort();
        names
    }
}

/// The names `vol` lists in `dir`, sorted.
fn listed<D: BlockDevice>(vol: &mut Volume<D>, dir: &str) -> Vec<String> {
    let dir = block_on(vol.open_dir(dir)).unwrap();
    let mut cursor = vol.cursor(dir);
    let mut name = Name::new();
    let mut names = Vec::new();
    while block_on(vol.next(&mut cursor, &mut name))
        .unwrap()
        .is_some()
    {
        names.push(name.as_str().to_string());
    }
    names.sort();
    names
}

/// The whole file at `path`, read `chunk` bytes at a time.
fn read_all<D: BlockDevice>(vol: &mut Volume<D>, path: &str, chunk: usize) -> Vec<u8> {
    let entry = block_on(vol.find(path)).unwrap();
    let mut file = vol.open(&entry).unwrap();
    let mut out = Vec::new();
    let mut buf = vec![0u8; chunk];
    loop {
        let n = block_on(vol.read(&mut file, &mut buf)).unwrap();
        if n == 0 {
            break;
        }
        out.extend_from_slice(&buf[..n]);
    }
    out
}

/// Every directory lists what was put in it, and every file reads back whole, in reads of
/// several sizes.
fn check(host: &Host) {
    let mut vol = block_on(Volume::mount(host.image())).unwrap();
    let mut dirs = vec![String::new()];
    dirs.extend(
        host.tree
            .iter()
            .filter(|(_, v)| v.is_none())
            .map(|(k, _)| k.clone()),
    );
    for dir in &dirs {
        assert_eq!(listed(&mut vol, dir), host.names_in(dir), "in {dir:?}");
    }
    for (path, bytes) in host
        .tree
        .iter()
        .filter_map(|(k, v)| v.as_ref().map(|b| (k, b)))
    {
        for chunk in [1, 7, 511, 512, 4096, 70_000] {
            assert!(
                read_all(&mut vol, path, chunk) == *bytes,
                "{path} in reads of {chunk}"
            );
        }
    }
}

/// A tree with long Turkish names, short names only (mtools writes a name that fits 8.3 in its
/// code page, 850, without a long name), nested directories and files of sizes around blocks and clusters.
/// (mtools writes a character past the BMP as one UTF-16 unit, so none is here: see
/// `a_long_name_past_the_bmp`.)
fn fill(host: &mut Host, cluster: usize) {
    host.put("README.TXT", content(1, 100));
    host.put("readme lower.txt", content(2, 0));
    host.mkdir("Finans ve Gelişim");
    host.put(
        "Finans ve Gelişim/İğdır şehri ve Çağrı.epub",
        content(3, 3 * cluster + 17),
    );
    host.put("döngü.bin", content(4, 511));
    host.mkdir("çok");
    host.mkdir("Kitaplar");
    host.mkdir("Kitaplar/Roman");
    for (i, len) in [
        1,
        511,
        512,
        513,
        cluster - 1,
        cluster,
        cluster + 1,
        5 * cluster,
        70_000,
    ]
    .into_iter()
    .enumerate()
    {
        host.put(
            &format!("Kitaplar/Roman/uzun dosya adı {i:02} — çok uzun bir başlık, gerçekten.epub"),
            content(10 + i as u32, len),
        );
    }
}

#[test]
fn volumes_of_both_types_list_and_read_back() {
    if !tools() {
        return;
    }
    // FAT16 and FAT32, with and without a partition table, clusters of 1, 4 and 8 blocks; one
    // FAT32 with fewer clusters than FAT32 should have, which is still FAT32.
    for (bits, kib, cluster, mbr) in [
        (16, 65_536, 4, false),
        (16, 32_768, 1, true),
        (32, 40_960, 1, false),
        (32, 40_960, 8, false),
        (32, 270_000, 8, true),
    ] {
        let mut host = Host::new(bits, kib, cluster, mbr);
        fill(&mut host, usize::from(cluster) * BLOCK);
        let vol = block_on(Volume::mount(host.image())).unwrap();
        assert_eq!(
            vol.kind(),
            if bits == 16 { Kind::Fat16 } else { Kind::Fat32 }
        );
        check(&host);
    }
}

#[test]
fn a_directory_over_many_clusters_and_deleted_entries() {
    if !tools() {
        return;
    }
    let mut host = Host::new(32, 40_960, 1, false);
    host.mkdir("çok");
    for i in 0..300 {
        host.put(
            &format!("çok/dosya numarası {i:03} uzun bir ad.txt"),
            content(i, (i % 7) as usize * 100),
        );
    }
    // Every third deleted: their entries stay, marked, with their long-name pieces.
    for i in (0..300).step_by(3) {
        host.delete(&format!("çok/dosya numarası {i:03} uzun bir ad.txt"));
    }
    check(&host);
}

#[test]
fn a_fragmented_file_reads_back() {
    if !tools() {
        return;
    }
    let mut host = Host::new(16, 32_768, 1, false);
    for i in 0..40 {
        host.put(&format!("küçük {i:02}.bin"), content(i, BLOCK));
    }
    for i in (0..40).step_by(2) {
        host.delete(&format!("küçük {i:02}.bin"));
    }
    host.put("büyük parçalı.bin", content(99, 60 * BLOCK + 3));
    // Its chain must jump, or the test shows nothing.
    let mut vol = block_on(Volume::mount(host.image())).unwrap();
    let entry = block_on(vol.find("büyük parçalı.bin")).unwrap();
    let (mut cluster, mut jumps) = (entry.cluster, 0);
    while let Some(next) = block_on(vol.next_cluster(cluster)).unwrap() {
        jumps += usize::from(next != cluster + 1);
        cluster = next;
    }
    assert!(jumps > 5, "{jumps} jumps");
    check(&host);
}

#[test]
fn seeks_read_from_where_they_go() {
    if !tools() {
        return;
    }
    let mut host = Host::new(32, 40_960, 1, false);
    let bytes = content(7, 40_000);
    host.put("ara.bin", bytes.clone());
    let mut vol = block_on(Volume::mount(host.image())).unwrap();
    let entry = block_on(vol.find("ara.bin")).unwrap();
    let mut file = vol.open(&entry).unwrap();
    let mut x = 12345u32;
    let mut buf = vec![0u8; 3000];
    for _ in 0..200 {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        let pos = x % 41_000;
        let len = (x >> 8) as usize % buf.len();
        file.seek(pos);
        let n = block_on(vol.read(&mut file, &mut buf[..len])).unwrap();
        let start = (pos as usize).min(bytes.len());
        let want = &bytes[start..(start + len).min(bytes.len())];
        assert_eq!(&buf[..n], want, "at {pos} for {len}");
        assert_eq!(file.position() as usize, start + n);
    }
}

#[test]
fn contiguous_clusters_come_in_one_call() {
    if !tools() {
        return;
    }
    let mut host = Host::new(32, 40_960, 1, false);
    host.put("ardışık kümeler.bin", content(5, 64 * 1024));
    let mut vol = block_on(Volume::mount(host.image())).unwrap();
    let entry = block_on(vol.find("ardışık kümeler.bin")).unwrap();
    let mut file = vol.open(&entry).unwrap();
    let mut buf = vec![0u8; 64 * 1024];
    let before = vol.dev.calls;
    assert_eq!(block_on(vol.read(&mut file, &mut buf)).unwrap(), buf.len());
    // 128 clusters of one block: the data in one call, the FAT in a block or two.
    assert!(
        vol.dev.calls - before <= 3,
        "{} calls",
        vol.dev.calls - before
    );
    assert!(buf == content(5, 64 * 1024));
}

#[test]
fn paths_are_found_ignoring_case() {
    if !tools() {
        return;
    }
    let mut host = Host::new(16, 32_768, 1, false);
    fill(&mut host, BLOCK);
    let mut vol = block_on(Volume::mount(host.image())).unwrap();
    let a = block_on(
        vol.find("/kitaplar/ROMAN/Uzun Dosya Adı 00 — çok uzun bir başlık, gerçekten.EPUB"),
    )
    .unwrap();
    let b = block_on(
        vol.find("Kitaplar/Roman/uzun dosya adı 00 — çok uzun bir başlık, gerçekten.epub"),
    )
    .unwrap();
    assert_eq!(a, b);
    // Each UTF-16 unit in upper case by SdFat's table: i is I, but ı stays ı and İ stays İ, so
    // neither matches i; ß's upper case is two letters, so it stays ß and matches no S.
    assert_eq!(
        block_on(vol.find("FINANS VE GELIŞIM")),
        block_on(vol.find("Finans ve Gelişim"))
    );
    assert!(block_on(vol.find("Finans ve Gelişim")).is_ok());
    assert!(block_on(vol.find("Finans ve Gelİşim")).is_err());
    assert!(block_on(vol.find("Finanß ve Gelişim")).is_err());
    assert_eq!(
        block_on(
            vol.find("Kitaplar/Roman/uzun dosya adi 00 — çok uzun bir başlık, gerçekten.epub")
        ),
        Err(Error::NotFound)
    );
    // Leading spaces, trailing dots and spaces are not part of the name looked up.
    assert_eq!(
        block_on(vol.find("  README.TXT. .")),
        block_on(vol.find("README.TXT"))
    );
    // The short name finds it too.
    let readme = block_on(vol.find("readme.txt")).unwrap();
    assert_eq!(&readme.short_name, b"README  TXT");
    assert_eq!(
        block_on(vol.find("Kitaplar/yok.epub")),
        Err(Error::NotFound)
    );
    assert_eq!(
        block_on(vol.find("README.TXT/x")),
        Err(Error::NotADirectory)
    );
    // A short name matches with its extension only.
    assert_eq!(block_on(vol.find("README.DOC")), Err(Error::NotFound));
    assert_eq!(block_on(vol.find("README")), Err(Error::NotFound));
    assert_eq!(block_on(vol.find("")), Err(Error::NotFound));
    assert_eq!(block_on(vol.open_dir("/")), Ok(vol.root()));
    let dir = block_on(vol.find("Kitaplar")).unwrap();
    assert_eq!(vol.open(&dir), Err(Error::NotAFile));
}

#[test]
fn no_volume_fat12_and_damage_are_errors() {
    let blank = Image {
        bytes: vec![0; 64 * 1024],
        calls: 0,
    };
    assert!(matches!(
        block_on(Volume::mount(blank)),
        Err(Error::NoVolume)
    ));
    if !tools() {
        return;
    }
    let small = Host::new(12, 1024, 1, false);
    assert!(matches!(
        block_on(Volume::mount(small.image())),
        Err(Error::Unsupported)
    ));

    // A chain that points back at the same cluster: the read stops with an error.
    let mut host = Host::new(16, 32_768, 1, false);
    host.put("döngü döngü.bin", content(8, 10 * BLOCK));
    let mut image = host.image();
    let (l, entry) = {
        let mut vol = block_on(Volume::mount(&mut image)).unwrap();
        (vol.layout, block_on(vol.find("döngü döngü.bin")).unwrap())
    };
    let second = entry.cluster + 1;
    let at = l.fat_start as usize * BLOCK + second as usize * 2;
    image.bytes[at..at + 2].copy_from_slice(&(second as u16).to_le_bytes());
    let mut vol = block_on(Volume::mount(&mut image)).unwrap();
    let mut file = vol.open(&entry).unwrap();
    let mut buf = vec![0u8; 10 * BLOCK];
    assert_eq!(block_on(vol.read(&mut file, &mut buf)), Err(Error::Corrupt));
    // A reserved value, not a cluster, in the chain.
    let first = l.fat_start as usize * BLOCK + entry.cluster as usize * 2;
    image.bytes[first..first + 2].copy_from_slice(&1u16.to_le_bytes());
    let mut vol = block_on(Volume::mount(&mut image)).unwrap();
    let mut file = vol.open(&entry).unwrap();
    assert_eq!(block_on(vol.read(&mut file, &mut buf)), Err(Error::Corrupt));
}

/// A long name with a character past the BMP, which UTF-16 keeps as two units.
#[test]
fn a_long_name_past_the_bmp() {
    let units: Vec<u16> = "kitap 📚.txt".encode_utf16().collect();
    let short = *b"KITAP~1 TXT";
    let mut raw = [0u8; 32];
    raw[0] = 0x41;
    raw[11] = 0x0F;
    raw[13] = dir::tests_support::checksum(&short);
    for (i, &at) in [1usize, 3, 5, 7, 9, 14, 16, 18, 20, 22, 24, 28, 30]
        .iter()
        .enumerate()
    {
        let unit = units
            .get(i)
            .copied()
            .unwrap_or(if i == units.len() { 0 } else { 0xFFFF });
        raw[at..at + 2].copy_from_slice(&unit.to_le_bytes());
    }
    let mut name = Name::new();
    assert!(dir::tests_support::one_piece(&raw, &short, &mut name));
    assert_eq!(name.as_str(), "kitap 📚.txt");
}

/// Where the root directory's raw entries start in a FAT16 image (no partition table).
fn fat16_root(image: &Image) -> usize {
    let b = &image.bytes;
    let reserved = usize::from(u16::from_le_bytes([b[14], b[15]]));
    let fat = usize::from(u16::from_le_bytes([b[22], b[23]]));
    (reserved + usize::from(b[16]) * fat) * BLOCK
}

/// The index of the first root entry `wanted` picks.
fn root_find(image: &Image, wanted: impl Fn(&[u8]) -> bool) -> usize {
    let root = fat16_root(image);
    (0..512)
        .find(|i| wanted(&image.bytes[root + i * 32..root + i * 32 + 32]))
        .unwrap()
}

/// The index of the root entry whose short name is `short`.
fn root_entry(image: &Image, short: &[u8; 11]) -> usize {
    let root = fat16_root(image);
    (0..512)
        .find(|i| &image.bytes[root + i * 32..root + i * 32 + 11] == short)
        .unwrap()
}

#[test]
fn long_name_pieces_out_of_order_are_not_a_name() {
    if !tools() {
        return;
    }
    let mut host = Host::new(16, 32_768, 1, false);
    // Pieces numbered n (with 0x40), n-1, ..., 1, then the short entry.
    host.put(
        "bir isim ki üç parçaya yayılır, uzun mu uzun.txt",
        content(1, 10),
    );
    let mut image = host.image();
    let root = fat16_root(&image);
    // The second piece numbered like the third: the long name is dropped, the short one shows.
    let first = root_find(&image, |e| e[11] == 0x0F && e[0] & 0x40 != 0);
    let second = root + (first + 1) * 32;
    let order = image.bytes[second] & 0x1F;
    assert!(order >= 2, "{order}");
    image.bytes[second] = order - 1;
    let mut vol = block_on(Volume::mount(&mut image)).unwrap();
    let dir = vol.root();
    let mut cursor = vol.cursor(dir);
    let mut name = Name::new();
    block_on(vol.next(&mut cursor, &mut name)).unwrap().unwrap();
    assert!(name.as_str().contains('~'), "{}", name.as_str());
}

#[test]
fn an_orphaned_long_name_is_not_taken() {
    if !tools() {
        return;
    }
    let mut host = Host::new(16, 32_768, 1, false);
    host.put("uzun adlı bir dosya.txt", content(1, 10));
    let mut image = host.image();
    let root = fat16_root(&image);
    // Its short entry deleted, and a copy of it (same name, so the same checksum) after.
    // (After the volume label and the long-name pieces.)
    let short = root_find(&image, |e| {
        e[0] != 0 && e[11] & 0x0F != 0x0F && e[11] & 0x08 == 0
    });
    let at = root + short * 32;
    let copy: Vec<u8> = image.bytes[at..at + 32].to_vec();
    image.bytes[at + 32..at + 64].copy_from_slice(&copy);
    image.bytes[at] = 0xE5;
    let mut vol = block_on(Volume::mount(&mut image)).unwrap();
    let dir = vol.root();
    let mut cursor = vol.cursor(dir);
    let mut name = Name::new();
    block_on(vol.next(&mut cursor, &mut name)).unwrap().unwrap();
    assert!(name.as_str().contains('~'), "{}", name.as_str());
}

#[test]
fn a_short_name_starting_with_0xe5_and_one_by_extension() {
    if !tools() {
        return;
    }
    let mut host = Host::new(16, 32_768, 1, false);
    host.put("README.TXT", content(1, 10));
    let mut image = host.image();
    let at = fat16_root(&image) + root_entry(&image, b"README  TXT") * 32;
    // 0x05 stands for 0xE5, which is a deleted entry's mark: σ in code page 437.
    image.bytes[at] = 0x05;
    let mut vol = block_on(Volume::mount(&mut image)).unwrap();
    assert_eq!(listed(&mut vol, ""), vec![String::from("σEADME.TXT")]);
    assert!(block_on(vol.find("σeadme.txt")).is_ok());
    // The short name matches only with its extension.
    assert_eq!(block_on(vol.find("σEADME.DOC")), Err(Error::NotFound));
    assert_eq!(block_on(vol.find("σEADME")), Err(Error::NotFound));
}

#[test]
fn fat16_reserved_values_end_no_chain() {
    if !tools() {
        return;
    }
    let mut host = Host::new(16, 32_768, 1, false);
    host.put("zincir zinciri.bin", content(2, 3 * BLOCK));
    let mut image = host.image();
    let (l, entry) = {
        let mut vol = block_on(Volume::mount(&mut image)).unwrap();
        (
            vol.layout,
            block_on(vol.find("zincir zinciri.bin")).unwrap(),
        )
    };
    // 0xFFF0 to 0xFFF7 are reserved or bad, not the chain's end (0xFFF8 on), even where a
    // file's chain may end.
    let last = entry.cluster + 2;
    let at = l.fat_start as usize * BLOCK + last as usize * 2;
    image.bytes[at..at + 2].copy_from_slice(&0xFFF5u16.to_le_bytes());
    let mut vol = block_on(Volume::mount(&mut image)).unwrap();
    assert_eq!(block_on(vol.next_cluster(last)), Err(Error::Corrupt));
    let at = l.fat_start as usize * BLOCK + entry.cluster as usize * 2;
    image.bytes[at..at + 2].copy_from_slice(&0xFFF5u16.to_le_bytes());
    let mut vol = block_on(Volume::mount(&mut image)).unwrap();
    let mut file = vol.open(&entry).unwrap();
    let mut buf = vec![0u8; 3 * BLOCK];
    assert_eq!(block_on(vol.read(&mut file, &mut buf)), Err(Error::Corrupt));
}

#[test]
fn fat32_uses_the_fat_its_flags_name_and_clusters_past_65535() {
    if !tools() {
        return;
    }
    // Clusters of one block on 40 MiB: a file after 36 MiB of filler starts past cluster 65535.
    let mut host = Host::new(32, 40_960, 1, false);
    host.put("dolgu.bin", content(3, 36 * 1024 * 1024));
    host.put("yüksek küme.bin", content(4, 5000));
    let mut image = host.image();
    let entry = {
        let mut vol = block_on(Volume::mount(&mut image)).unwrap();
        block_on(vol.find("yüksek küme.bin")).unwrap()
    };
    assert!(entry.cluster > 0xFFFF, "cluster {}", entry.cluster);
    assert!(
        read_all(
            &mut block_on(Volume::mount(host.image())).unwrap(),
            "yüksek küme.bin",
            4096
        ) == content(4, 5000)
    );
    // Mirroring off, the second FAT active, the first wiped: reads go through the second.
    let b = &mut image.bytes;
    let reserved = usize::from(u16::from_le_bytes([b[14], b[15]]));
    let fat = u32::from_le_bytes([b[36], b[37], b[38], b[39]]) as usize;
    b[40] = 0x81;
    b[reserved * BLOCK + 8..(reserved + fat) * BLOCK].fill(0);
    let mut vol = block_on(Volume::mount(&mut image)).unwrap();
    assert!(read_all(&mut vol, "yüksek küme.bin", 4096) == content(4, 5000));
}

/// A copy of a FAT16 image's boot sector field at `at` set to `value`, mounted.
fn mount_with(image: &Image, at: usize, value: &[u8]) -> Result<Kind, Error<()>> {
    let mut bytes = image.bytes.clone();
    bytes[at..at + value.len()].copy_from_slice(value);
    block_on(Volume::mount(Image { bytes, calls: 0 })).map(|v| v.kind())
}

#[test]
fn boot_sectors_that_do_not_add_up_are_errors() {
    if !tools() {
        return;
    }
    let host = Host::new(16, 32_768, 1, false);
    let image = host.image();
    assert_eq!(mount_with(&image, 0, &[0xEB]), Ok(Kind::Fat16));
    // A total past the end of a u32 block number, from where the volume starts.
    let mut big = image.bytes.clone();
    big[19..21].fill(0);
    big[32..36].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        block_on(Volume::mount(Image {
            bytes: big,
            calls: 0
        })),
        Err(Error::Corrupt)
    ));
    // FATs bigger than the volume; a FAT too small for the clusters; FAT16 without a root.
    assert_eq!(
        mount_with(&image, 22, &0xFFFFu16.to_le_bytes()),
        Err(Error::Corrupt)
    );
    assert_eq!(
        mount_with(&image, 22, &1u16.to_le_bytes()),
        Err(Error::Corrupt)
    );
    let roomy = Host::new(16, 65_536, 4, false);
    assert_eq!(mount_with(&roomy.image(), 17, &[0, 0]), Err(Error::Corrupt));
    // A volume longer than its partition.
    let host = Host::new(16, 32_768, 1, true);
    let mut image = host.image();
    image.bytes[0x1CA..0x1CE].copy_from_slice(&1000u32.to_le_bytes());
    assert!(matches!(
        block_on(Volume::mount(image)),
        Err(Error::Corrupt)
    ));
    // A FAT32 volume at block 2048 whose length, with a FAT big enough for its clusters, ends
    // past the last u32 block number.
    let host = Host::new(32, 40_960, 1, true);
    let mut image = host.image();
    image.bytes[0x1CA..0x1CE].copy_from_slice(&u32::MAX.to_le_bytes());
    let boot = 2048 * BLOCK;
    image.bytes[boot + 13] = 128;
    image.bytes[boot + 32..boot + 36].copy_from_slice(&0xFFFF_FF00u32.to_le_bytes());
    image.bytes[boot + 36..boot + 40].copy_from_slice(&0x4_0000u32.to_le_bytes());
    assert!(matches!(
        block_on(Volume::mount(image)),
        Err(Error::Corrupt)
    ));
}

#[test]
fn exfat_is_unsupported() {
    // An exFAT boot sector, alone and in a partition of its type.
    let mut bytes = vec![0u8; 4096 * BLOCK];
    bytes[0..3].copy_from_slice(&[0xEB, 0x76, 0x90]);
    bytes[3..11].copy_from_slice(b"EXFAT   ");
    bytes[510..512].copy_from_slice(&[0x55, 0xAA]);
    let alone = Image {
        bytes: bytes.clone(),
        calls: 0,
    };
    assert!(matches!(
        block_on(Volume::mount(alone)),
        Err(Error::Unsupported)
    ));
    let mut mbr = vec![0u8; 2048 * BLOCK];
    mbr[0x1BE + 4] = 0x07;
    mbr[0x1BE + 8..0x1BE + 12].copy_from_slice(&2048u32.to_le_bytes());
    mbr[0x1BE + 12..0x1BE + 16].copy_from_slice(&4096u32.to_le_bytes());
    mbr[510..512].copy_from_slice(&[0x55, 0xAA]);
    mbr.extend_from_slice(&bytes);
    let parted = Image {
        bytes: mbr,
        calls: 0,
    };
    assert!(matches!(
        block_on(Volume::mount(parted)),
        Err(Error::Unsupported)
    ));
}

#[test]
fn a_fat_partition_after_an_exfat_or_ntfs_one_is_mounted() {
    if !tools() {
        return;
    }
    let mut host = Host::new(32, 40_960, 1, true);
    host.put("README.TXT", content(1, 10));
    let mut image = host.image();
    // The FAT32 entry moved to the second slot; the first a 0x07 one over blocks 1 to 2047.
    let fat32: Vec<u8> = image.bytes[0x1BE..0x1CE].to_vec();
    image.bytes[0x1CE..0x1DE].copy_from_slice(&fat32);
    let first = &mut image.bytes[0x1BE..0x1CE];
    first.fill(0);
    first[4] = 0x07;
    first[8..12].copy_from_slice(&1u32.to_le_bytes());
    first[12..16].copy_from_slice(&2047u32.to_le_bytes());
    let mut vol = block_on(Volume::mount(&mut image)).unwrap();
    assert_eq!(vol.kind(), Kind::Fat32);
    assert!(read_all(&mut vol, "README.TXT", 512) == content(1, 10));
}

#[test]
fn a_directory_entry_without_a_cluster_and_a_file_bigger_than_the_volume() {
    if !tools() {
        return;
    }
    let mut host = Host::new(16, 32_768, 1, false);
    host.mkdir("Kitaplar");
    host.put("Kitaplar/bir.txt", content(1, 10));
    host.put("README.TXT", content(2, 10));
    let mut image = host.image();
    let root = fat16_root(&image);
    // Cluster 0 is the root's: a directory entry naming it is damaged, not the root.
    let dir = root + root_entry(&image, b"KITAPLAR   ") * 32;
    image.bytes[dir + 26..dir + 28].fill(0);
    // A size no volume this big holds.
    let file = root + root_entry(&image, b"README  TXT") * 32;
    image.bytes[file + 28..file + 32].copy_from_slice(&u32::MAX.to_le_bytes());
    let mut vol = block_on(Volume::mount(&mut image)).unwrap();
    let entry = block_on(vol.find("Kitaplar")).unwrap();
    assert!(entry.is_dir() && entry.dir().is_none());
    assert_eq!(entry.size(), 0);
    assert_eq!(block_on(vol.find("Kitaplar/bir.txt")), Err(Error::Corrupt));
    assert_eq!(block_on(vol.open_dir("Kitaplar")), Err(Error::Corrupt));
    let readme = block_on(vol.find("README.TXT")).unwrap();
    assert_eq!(readme.size(), u32::MAX);
    assert_eq!(vol.open(&readme), Err(Error::Corrupt));
}

#[test]
fn lookups_from_a_directory() {
    if !tools() {
        return;
    }
    let mut host = Host::new(16, 32_768, 1, false);
    fill(&mut host, BLOCK);
    let mut vol = block_on(Volume::mount(host.image())).unwrap();
    let dir = block_on(vol.open_dir("Kitaplar")).unwrap();
    let a = block_on(vol.find_from(
        dir,
        "Roman/uzun dosya adı 01 — çok uzun bir başlık, gerçekten.epub",
    ))
    .unwrap();
    let b = block_on(
        vol.find("Kitaplar/Roman/uzun dosya adı 01 — çok uzun bir başlık, gerçekten.epub"),
    )
    .unwrap();
    assert_eq!(a, b);
    assert_eq!(a.size(), 511);
    assert_eq!(
        block_on(vol.find_from(dir, "README.TXT")),
        Err(Error::NotFound)
    );
}

#[test]
fn an_extension_starting_with_0x05_is_not_0xe5() {
    if !tools() {
        return;
    }
    let mut host = Host::new(16, 32_768, 1, false);
    host.put("README.TXT", content(1, 10));
    let mut image = host.image();
    let at = fat16_root(&image) + root_entry(&image, b"README  TXT") * 32;
    image.bytes[at + 8] = 0x05;
    let mut vol = block_on(Volume::mount(&mut image)).unwrap();
    let names = listed(&mut vol, "");
    assert_eq!(names.len(), 1);
    assert!(
        names[0].starts_with("README.") && !names[0].contains('σ'),
        "{}",
        names[0]
    );
}

#[test]
fn upper_case_is_sdfat_s() {
    use crate::upcase::upcase;
    for (unit, upper) in [
        ('a', 'A'),
        ('z', 'Z'),
        ('{', '{'),
        ('ü', 'Ü'),
        ('ş', 'Ş'),
        ('ğ', 'Ğ'),
        // A run of pairs starting at an odd unit.
        ('ĺ', 'Ĺ'),
        ('Ĺ', 'Ĺ'),
        ('ı', 'ı'),
        ('İ', 'İ'),
        ('ÿ', 'Ÿ'),
        ('µ', 'µ'),
        ('ß', 'ß'),
        ('σ', 'Σ'),
        ('ς', 'Σ'),
        ('я', 'Я'),
        ('ⴀ', 'Ⴀ'),
        ('ａ', 'Ａ'),
    ] {
        assert_eq!(upcase(unit as u16), upper as u16, "{unit}");
    }
    // Not a letter, and a surrogate: themselves.
    assert_eq!(upcase(0xD83D), 0xD83D);
    assert_eq!(upcase(0xFFFF), 0xFFFF);
}
