use std::{fs::{self, File}, io::{self, Seek}, convert::TryFrom, path::{Path, PathBuf}};

use anyhow::Result;

fn create_fat_filesystem(fat_path: &Path, efi_file: &Path) -> Result<()> {
    let efi_size = fs::metadata(&efi_file)?.len();
    let mb = 1024 * 1024;
    let efi_size_rounded = ((efi_size - 1) / mb + 1) * mb;

    let fat_file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&fat_path)?;
    fat_file.set_len(efi_size_rounded)?;

    let format_options = fatfs::FormatVolumeOptions::new();
    fatfs::format_volume(&fat_file, format_options)?;
    let filesystem = fatfs::FileSystem::new(&fat_file, fatfs::FsOptions::new())?;

    let root_dir = filesystem.root_dir();
    root_dir.create_dir("efi")?;
    root_dir.create_dir("efi/boot")?;
    let mut bootx64 = root_dir.create_file("efi/boot/bootx64.efi")?;
    bootx64.truncate()?;
    io::copy(&mut fs::File::open(&efi_file)?, &mut bootx64)?;

    Ok(())
}

fn create_gpt_disk(disk_path: &Path, fat_image: &Path) -> Result<()> {
    let mut disk = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .read(true)
        .write(true)
        .open(&disk_path)
        .expect("ERROR: Unable to Create Disk");

    let partition_size: u64 = fs::metadata(&fat_image)?.len();
    let disk_size = partition_size + 1024 * 64;
    disk.set_len(disk_size)?;

    let mbr = gpt::mbr::ProtectiveMBR::with_lb_size(
        u32::try_from((disk_size / 512) - 1).unwrap_or(0xFF_FF_FF_FF),
    );
    mbr.overwrite_lba0(&mut disk)?;

    let block_size = gpt::disk::LogicalBlockSize::Lb512;
    let mut gpt = gpt::GptConfig::new()
        .writable(true)
        .initialized(false)
        .logical_block_size(block_size)
        .create_from_device(Box::new(&mut disk), None)?;
    gpt.update_partitions(Default::default())?;

    let partition_id = gpt
        .add_partition("boot", partition_size, gpt::partition_types::EFI, 0, None)?;
    let partition = gpt.partitions().get(&partition_id).expect("Unable to Get Partition");
    let start_offset = partition.bytes_start(block_size)?;

    gpt.write()?;

    disk.seek(io::SeekFrom::Start(start_offset))?;
    io::copy(&mut File::open(&fat_image)?, &mut disk)?;

    Ok(())
}

fn main() -> Result<()> {
    let mut args = std::env::args();
    let _exe_name = args.next().expect("ERROR: Unable to Parse Command Line Arguments");
    let efi_path = PathBuf::from(args.next().expect("ERROR: No File Specified"));
    let fat_path = efi_path.with_extension("fat");
    let disk_path = fat_path.with_extension("gdt");

    create_fat_filesystem(&fat_path, &efi_path)?;
    create_gpt_disk(&disk_path, &fat_path)?;

    Ok(())
}