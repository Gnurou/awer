use std::fmt;
use std::fs::File;
use std::io;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;

use enumn::N;
use tracing::debug;
use zerocopy::big_endian::U16;
use zerocopy::big_endian::U32;
use zerocopy::FromBytes;

use crate::audio::MusicModule;
use crate::audio::SoundSample;

#[derive(Clone, Copy, PartialEq, Eq, Debug, N)]
pub enum ResType {
    // Audio samples.
    // All entries of this type are loaded by the loadresource opcode.
    Sound = 0,
    // Music.
    // All entries of this type are loaded by the loadresource opcode.
    Music = 1,
    // Full-screen bitmaps used for the title screen as well as backgrounds for
    // some scenes. Apparently the game was on a rush to be finished and these
    // static backgrounds got added instead of being generated from polygons.
    // Loaded by the loadresource opcode.
    Bitmap = 2,
    // Groups of 64 palettes of 16 colors each (2 bytes per color, encoding
    // still a bit obscure).
    // All entries of this type are referenced from the scenes list.
    Palette = 3,
    // Bytecode for the virtual machine.
    // All entries of this type are referenced from the scenes list.
    Bytecode = 4,
    // Polygons for cinematic scenes.
    // All entries of this type are referenced from the scenes list.
    Cinematic = 5,
    // Polygons for in-game animations.
    Poly = 6,
}

impl fmt::Display for ResType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

#[derive(FromBytes)]
#[repr(C, packed)]
struct UnpackFooter {
    chk: U32,
    crc: U32,
    data_size: U32,
}

struct UnpackContext<'a> {
    // Data array, of the size of the unpacked data, filled
    // with the packed data up to i_buf
    data: &'a mut [u8],
    // Current CRC, must be zero when unpacking finishes
    crc: u32,
    // Current chunk of data
    chk: u32,
    // Input (packed) data pointer
    i_buf: usize,
    // Output (unpacked) data pointer
    o_buf: usize,
}

impl<'a> UnpackContext<'a> {
    // Create a new unpacking context. The data buffer is large enough to
    // contain the whole uncompressed data, but is only filled with compressed
    // data up to packed_len. The data will then be uncompressed in-place.
    fn new(data: &'a mut [u8], packed_len: usize) -> io::Result<UnpackContext<'a>> {
        assert!(data.len() >= packed_len);
        let footer_start = packed_len - std::mem::size_of::<UnpackFooter>();
        assert_eq!(footer_start % 4, 0);
        let footer = UnpackFooter::read_from_bytes(&data[footer_start..packed_len]).unwrap();
        let data_size = footer.data_size.get() as usize;
        assert_eq!(data_size, data.len());
        let crc = footer.crc.get() ^ footer.chk.get();

        Ok(UnpackContext {
            data,
            crc,
            chk: footer.chk.get(),
            i_buf: footer_start,
            o_buf: data_size,
        })
    }

    fn rcr(&mut self) -> bool {
        let rcf = (self.chk & 1) == 1;
        self.chk >>= 1;
        rcf
    }

    fn next_bit(&mut self) -> bool {
        let cf = self.rcr();
        // We still have data, return the bit that we got
        if self.chk != 0 {
            return cf;
        }

        // We need to read new data from the packed buffer
        assert_ne!(self.i_buf, 0);
        self.i_buf -= 4;
        self.chk = u32::from_be_bytes(self.data[self.i_buf..self.i_buf + 4].try_into().unwrap());
        self.crc ^= self.chk;
        // Get the first bit of our 32-bit word, and insert a 1 in the MSB to
        // mark the end of the word (self.chk will be == 0 after reading that
        // bit).
        let cf = self.rcr();
        self.chk |= 1 << 31;
        cf
    }

    // Get the integer made of the next x bits
    fn get_code(&mut self, num_bits: u8) -> u16 {
        let mut c = 0u16;
        for _ in 0..num_bits {
            c <<= 1;
            c |= self.next_bit() as u16;
        }
        c
    }

    fn dec_unk1(&mut self, num_bits: u8, add_count: u16) {
        let count = self.get_code(num_bits) + add_count;

        for _ in 0..count {
            assert!(self.o_buf >= self.i_buf);
            self.o_buf -= 1;
            self.data[self.o_buf] = self.get_code(8) as u8;
        }
    }

    fn dec_unk2(&mut self, num_bits: u8, add_count: u16) {
        let offset = self.get_code(num_bits) as usize;
        let count = add_count;

        for _ in 0..count {
            assert!(self.o_buf >= self.i_buf);
            self.o_buf -= 1;
            self.data[self.o_buf] = self.data[self.o_buf + offset];
        }
    }

    fn unpack(mut self) -> io::Result<()> {
        loop {
            if self.next_bit() {
                match self.get_code(2) {
                    3 => self.dec_unk1(8, 9),
                    c @ 0..=1 => self.dec_unk2((c + 9) as u8, c + 3),
                    _ => {
                        let size = self.get_code(8);
                        self.dec_unk2(12, size + 1)
                    }
                }
            } else if self.next_bit() {
                self.dec_unk2(8, 2)
            } else {
                self.dec_unk1(3, 1)
            }
            if self.o_buf == 0 {
                break;
            }
        }

        match self.crc {
            0 => Ok(()),
            _ => Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid CRC")),
        }
    }
}

/// An entry in the `memlist.bin` file.
#[derive(FromBytes)]
#[repr(C, packed)]
struct MemlistEntry {
    state: u8,
    res_type: u8,
    _unk1: U16,
    _unk2: U16,
    rank_num: u8,
    bank_id: u8,
    bank_offset: U32,
    _unk3: U16,
    psize: U16,
    _unk4: U16,
    size: U16,
}

/// A validated entry of the `memlist.bin` file.
///
/// Its `res_type` member has been validated, and unneeded members are removed.
#[derive(Debug)]
#[allow(dead_code)]
struct MemEntry {
    res_type: ResType,
    // not sure what this is?
    rank_num: u8,
    bank_id: u8,
    bank_offset: u32,
    packed_size: usize,
    size: usize,
}

impl TryFrom<&MemlistEntry> for MemEntry {
    type Error = std::io::Error;

    fn try_from(entry: &MemlistEntry) -> Result<Self, Self::Error> {
        Ok(Self {
            res_type: ResType::n(entry.res_type).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "Invalid resource type!")
            })?,
            rank_num: entry.rank_num,
            bank_id: entry.bank_id,
            bank_offset: entry.bank_offset.get(),
            packed_size: entry.psize.get() as usize,
            size: entry.size.get() as usize,
        })
    }
}

pub struct LoadedResource {
    pub res_type: ResType,
    pub data: Vec<u8>,
}

impl LoadedResource {
    pub fn into_sound(self) -> Option<Box<SoundSample>> {
        match self.res_type {
            ResType::Sound => Some(unsafe { SoundSample::from_raw_resource(self.data) }),
            _ => None,
        }
    }

    pub fn into_music(self) -> Option<Box<MusicModule>> {
        match self.res_type {
            ResType::Music => Some(unsafe { MusicModule::from_raw_resource(self.data) }),
            _ => None,
        }
    }
}

impl MemEntry {
    fn load(&self) -> io::Result<Vec<u8>> {
        // Some resources happen to be empty but are still referenced during the game...
        if self.size == 0 {
            return Ok(Vec::new());
        }

        let mut file = File::open(format!("bank{:02x}", self.bank_id))?;
        file.seek(SeekFrom::Start(self.bank_offset as u64))?;

        let mut data = vec![0u8; self.size];
        file.read_exact(&mut data[..self.packed_size])?;

        if self.size > self.packed_size {
            let unpack_ctx = UnpackContext::new(&mut data[..], self.packed_size)?;
            unpack_ctx.unpack()?;
        }

        Ok(data)
    }

    // Bitmap data is 4 bits per pixel (16 colors), where each bit is stored in
    // a different plane. This means the 32000 bytes data is made of four 8000
    // bytes planes, which this function reconstitutes into an 8bpp (but still
    // 16 colors), linear bitmap buffer.
    #[allow(dead_code)]
    fn fixup_bitmap(data: &[u8]) -> Vec<u8> {
        let plane_length = data.len() / 4;
        let planes: Vec<&[u8]> = data.chunks(plane_length).collect();
        let mut res = vec![0u8; data.len() * 2];

        // Each byte contains one bit for 8 pixels
        for (i, chunk) in res.chunks_mut(8).enumerate() {
            // First get the pixel data of each plane for our 8 pixels
            let pixel_data = [planes[0][i], planes[1][i], planes[2][i], planes[3][i]];

            // Gather each bit from each plane for each of our 8 pixels
            for b in 0..8 {
                chunk[7 - b] = ((pixel_data[0] >> b as u8) & 0b1)
                    | ((pixel_data[1] >> b as u8) & 0b1) << 1
                    | ((pixel_data[2] >> b as u8) & 0b1) << 2
                    | ((pixel_data[3] >> b as u8) & 0b1) << 3;
            }
        }

        res
    }
}

#[allow(dead_code)]
pub struct ResourceManager {
    resources: Vec<MemEntry>,
}

impl ResourceManager {
    // TODO: change constructor to take a path to data, and return an error if the memlist cannot
    // be built
    pub fn new() -> io::Result<ResourceManager> {
        let mut ret = ResourceManager {
            resources: Vec::new(),
        };
        ret.load_mementries()?;
        Ok(ret)
    }

    fn load_mementries(&mut self) -> io::Result<()> {
        let mut file = File::open("memlist.bin").expect("Cannot open memlist.bin!");

        loop {
            let entry = MemlistEntry::read_from_io(&mut file)?;

            match entry.state {
                0 => (),
                0xff => break,
                _ => return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid state!")),
            };

            let validated_entry = MemEntry::try_from(&entry)?;

            debug!(
                "Resource 0x{:02x} of type {} size {}",
                self.resources.len(),
                entry.res_type,
                entry.size
            );

            self.resources.push(validated_entry);
        }
        Ok(())
    }

    /// Returns the resource type and data of resource entry `index`, loading it if necessary.
    pub fn load_resource(&self, index: usize) -> io::Result<LoadedResource> {
        let res = self
            .resources
            .get(index)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Resource does not exist!"))?;

        Ok(LoadedResource {
            res_type: res.res_type,
            data: res.load()?,
        })
    }

    fn show_stats_for(&self, res_type: ResType) {
        let (nb_res, psize, size) = self
            .resources
            .iter()
            .filter(|x| x.res_type == res_type)
            .fold((0usize, 0usize, 0usize), |p, x| {
                (p.0 + 1, p.1 + x.packed_size, p.2 + x.size)
            });
        println!(
            "{}: {} entries, size {} -> {}",
            res_type, nb_res, psize, size
        );
    }

    pub fn show_stats(&self) {
        #[derive(Clone, Copy)]
        #[allow(dead_code)]
        struct Stats {
            nb_resources: usize,
            packed_size: usize,
            size: usize,
        }

        let mut stats = [Stats {
            nb_resources: 0,
            packed_size: 0,
            size: 0,
        }; 7];
        for res in self.resources.iter() {
            let stat = &mut stats[res.res_type as usize];
            stat.nb_resources += 1;
            stat.packed_size += res.packed_size;
            stat.size += res.size;
        }

        self.show_stats_for(ResType::Sound);
        self.show_stats_for(ResType::Music);
        self.show_stats_for(ResType::Bitmap);
        self.show_stats_for(ResType::Palette);
        self.show_stats_for(ResType::Bytecode);
        self.show_stats_for(ResType::Cinematic);
        self.show_stats_for(ResType::Poly);
    }

    pub fn list_resources(&self) {
        for (i, resource_info) in self.resources.iter().enumerate() {
            println!("Entry 0x{:02x}: {:?}", i, resource_info);
        }

        println!("Entries stats by type:");
        self.show_stats();
    }

    pub fn dump_resources(&mut self) -> io::Result<()> {
        for i in 1..self.resources.len() {
            let resource = &self.resources[i];
            let data = self.load_resource(i)?.data;

            debug!(
                "Entry 0x{:x} of type {} loaded: {} ({}) bytes @{:1x},0x{:08x}",
                i,
                resource.res_type,
                resource.size,
                resource.packed_size,
                resource.bank_id,
                resource.bank_offset
            );

            const DUMPED_RESOURCES_DIR: &str = "resources";
            match std::fs::create_dir(DUMPED_RESOURCES_DIR) {
                Ok(()) => (),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
                Err(e) => return Err(e),
            }

            use std::io::Write;

            match resource.res_type {
                // for f in (ls img_*.dat); convert -size 320x200+0 -depth 8 gray:$f $f.png; end
                ResType::Bitmap => {
                    let mut file =
                        File::create(format!("{}/img_{:02x}.dat", DUMPED_RESOURCES_DIR, i))?;
                    file.write_all(
                        &MemEntry::fixup_bitmap(&data)
                            .iter()
                            .map(|x| x << 4)
                            .collect::<Vec<u8>>(),
                    )?;
                }
                ResType::Bytecode => {
                    let mut file =
                        File::create(format!("{}/code_{:02x}.dat", DUMPED_RESOURCES_DIR, i))?;
                    file.write_all(&data)?;
                }
                ResType::Cinematic => {
                    let mut file =
                        File::create(format!("{}/cine_{:02x}.dat", DUMPED_RESOURCES_DIR, i))?;
                    file.write_all(&data)?;
                }
                ResType::Sound => {
                    let mut file =
                        File::create(format!("{}/sound_{:02x}.dat", DUMPED_RESOURCES_DIR, i))?;
                    file.write_all(&data)?;
                }
                ResType::Music => {
                    let mut file =
                        File::create(format!("{}/music_{:02x}.dat", DUMPED_RESOURCES_DIR, i))?;
                    file.write_all(&data)?;
                }
                ResType::Palette => {
                    let mut file =
                        File::create(format!("{}/palette_{:02x}.dat", DUMPED_RESOURCES_DIR, i))?;
                    file.write_all(&data)?;
                }
                ResType::Poly => {
                    let mut file =
                        File::create(format!("{}/poly_{:02x}.dat", DUMPED_RESOURCES_DIR, i))?;
                    file.write_all(&data)?;
                }
            };
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_res() -> io::Result<()> {
        let resman = ResourceManager::new()?;
        assert_ne!(resman.resources.len(), 0);

        for i in 1..resman.resources.len() {
            let expected_size = resman.resources[i].size;
            let resource = resman.load_resource(i)?;
            assert_eq!(expected_size, resource.data.len());
        }

        Ok(())
    }
}
