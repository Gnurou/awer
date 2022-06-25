extern crate byteorder;

use std::{
    fmt,
    fs::File,
    io::{self, Read, Seek, SeekFrom},
};

use byteorder::{ReadBytesExt, BE};
use enumn::N;
use log::{debug, info};

use crate::audio::SoundSample;

#[derive(Clone, Copy, PartialEq, Debug, N)]
pub enum ResType {
    // Audio samples.
    // All entries of this type are loaded by the loadresource opcode.
    Sound,
    // Music.
    // All entries of this type are loaded by the loadresource opcode.
    Music,
    // Full-screen bitmaps used for the title screen as well as backgrounds for
    // some scenes. Apparently the game was on a rush to be finished and these
    // static backgrounds got added instead of being generated from polygons.
    // Loaded by the loadresource opcode.
    Bitmap,
    // Groups of 64 palettes of 16 colors each (2 bytes per color, encoding
    // still a bit obscure).
    // All entries of this type are referenced from the scenes list.
    Palette,
    // Bytecode for the virtual machine.
    // All entries of this type are referenced from the scenes list.
    Bytecode,
    // Polygons for cinematic scenes.
    // All entries of this type are referenced from the scenes list.
    Cinematic,
    // Not sure what this is yet, but seems like an alternative video segment.
    // There is only one entry of this type and it is referenced from the
    // scenes list.
    Unknown,
}

impl fmt::Display for ResType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
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
    fn new(data: &'a mut [u8], packed_len: usize) -> UnpackContext {
        assert!(data.len() >= packed_len);
        let mut i_buf = packed_len;
        assert_eq!(i_buf % 4, 0);
        i_buf -= 4;
        let data_size = (&data[i_buf..i_buf + 4]).read_u32::<BE>().unwrap() as usize;
        assert_eq!(data_size, data.len());
        i_buf -= 4;
        let crc = (&data[i_buf..i_buf + 4]).read_u32::<BE>().unwrap();
        i_buf -= 4;
        let chk = (&data[i_buf..i_buf + 4]).read_u32::<BE>().unwrap();
        let crc = crc ^ chk;

        UnpackContext {
            data,
            crc,
            chk,
            i_buf,
            o_buf: data_size,
        }
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
        self.chk = (&self.data[self.i_buf..self.i_buf + 4])
            .read_u32::<BE>()
            .unwrap();
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

#[allow(dead_code)]
struct MemEntryInfo {
    // not sure what this is?
    rank_num: u8,
    bank_id: u8,
    bank_offset: u32,
    packed_size: usize,
    size: usize,
}

#[derive(PartialEq)]
enum MemEntryState {
    NotLoaded,
    Loaded(Vec<u8>),
}

pub struct MemEntry {
    pub res_type: ResType,
    state: MemEntryState,
    info: MemEntryInfo,
}

pub struct LoadedResource<'a> {
    pub res_type: ResType,
    pub data: &'a Vec<u8>,
}

impl<'a> LoadedResource<'a> {
    pub fn into_sound(self) -> Option<Box<SoundSample>> {
        match self.res_type {
            ResType::Sound => Some(unsafe { SoundSample::from_raw_resource(self.data.clone()) }),
            _ => None,
        }
    }
}

impl MemEntry {
    fn load(&mut self) -> io::Result<()> {
        // Some resources happen to be empty but are still referenced during the game...
        if self.info.size == 0 {
            self.state = MemEntryState::Loaded(vec![]);
            return Ok(());
        }

        let mut file = File::open(format!("bank{:02x}", self.info.bank_id))?;
        file.seek(SeekFrom::Start(self.info.bank_offset as u64))?;

        let mut data = vec![0u8; self.info.size];
        file.read_exact(&mut data[..self.info.packed_size])?;

        if self.info.size > self.info.packed_size {
            let unpack_ctx = UnpackContext::new(&mut data[..], self.info.packed_size as usize);
            unpack_ctx.unpack()?;
        }

        self.state = MemEntryState::Loaded(data);

        Ok(())
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
    // TODO change constructor to take a path to data, and return an error if the memlist cannot be built
    pub fn new() -> io::Result<ResourceManager> {
        let mut ret = ResourceManager {
            resources: Vec::new(),
        };
        ret.load_mementries()?;
        ret.show_stats();
        Ok(ret)
    }

    fn load_mementries(&mut self) -> io::Result<()> {
        let mut file = File::open("memlist.bin").expect("Cannot open memlist.bin!");

        loop {
            // This file was supposed to be directly read into data structures, hence the "empty"
            // bits which used to be zero-initialized pointers.
            let state = file.read_u8()?;
            let res_type = file.read_u8()?;
            let _ = file.read_u16::<BE>()?;
            let _ = file.read_u16::<BE>()?;
            let rank_num = file.read_u8()?;
            let bank_id = file.read_u8()?;
            let bank_offset = file.read_u32::<BE>()?;
            let _ = file.read_u16::<BE>()?;
            let psize = file.read_u16::<BE>()?;
            let _ = file.read_u16::<BE>()?;
            let size = file.read_u16::<BE>()?;

            match state {
                0 => (),
                0xff => break,
                _ => return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid state!")),
            };

            let res_type = ResType::n(res_type).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "Invalid resource type!")
            })?;

            debug!(
                "Resource 0x{:02x} of type {} size {}",
                self.resources.len(),
                res_type,
                size
            );

            self.resources.push(MemEntry {
                res_type,
                state: MemEntryState::NotLoaded,
                info: MemEntryInfo {
                    rank_num,
                    bank_id,
                    bank_offset,
                    packed_size: psize as usize,
                    size: size as usize,
                },
            });
        }
        Ok(())
    }

    /// Returns the resource type and data of resource entry `index` if it is already loaded.
    ///
    /// If the entry is not already loaded, no attempt to load it is done and `None` is returned.
    pub fn get_resource(&self, index: usize) -> Option<LoadedResource> {
        let res = self.resources.get(index)?;

        match &res.state {
            MemEntryState::Loaded(data) => Some(LoadedResource {
                res_type: res.res_type,
                data,
            }),
            MemEntryState::NotLoaded => None,
        }
    }

    /// Returns the resource type and data of resource entry `index`, loading it if necessary.
    pub fn load_resource(&mut self, index: usize) -> io::Result<LoadedResource> {
        let res = self
            .resources
            .get_mut(index)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Resource does not exist!"))?;

        if matches!(res.state, MemEntryState::NotLoaded) {
            info!(
                "Loading resource 0x{:02x} of type {}, size {}",
                index, res.res_type, res.info.size
            );
            res.load()?;
        }

        self.get_resource(index).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "Resource unavailable after loading",
            )
        })
    }

    fn show_stats_for(&self, res_type: ResType) {
        let (nb_res, psize, size) = self
            .resources
            .iter()
            .filter(|x| x.res_type == res_type)
            .fold((0usize, 0usize, 0usize), |p, x| {
                (p.0 + 1, p.1 + x.info.packed_size, p.2 + x.info.size)
            });
        info!(
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
            stat.packed_size += res.info.packed_size as usize;
            stat.size += res.info.size as usize;
        }

        self.show_stats_for(ResType::Sound);
        self.show_stats_for(ResType::Music);
        self.show_stats_for(ResType::Bitmap);
        self.show_stats_for(ResType::Palette);
        self.show_stats_for(ResType::Bytecode);
        self.show_stats_for(ResType::Cinematic);
        self.show_stats_for(ResType::Unknown);
    }

    pub fn dump_resources(&mut self) -> io::Result<()> {
        for i in 1..self.resources.len() {
            let _ = self.load_resource(i)?;
            let resource = &self.resources[i];

            debug!(
                "Entry 0x{:x} of type {} loaded: {} ({}) bytes @{:1x},0x{:08x}",
                i,
                resource.res_type,
                resource.info.size,
                resource.info.packed_size,
                resource.info.bank_id,
                resource.info.bank_offset
            );

            const DUMPED_RESOURCES_DIR: &str = "resources";
            match std::fs::create_dir(DUMPED_RESOURCES_DIR) {
                Ok(()) => (),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
                Err(e) => return Err(e),
            }

            use std::io::Write;

            let data = match &resource.state {
                MemEntryState::Loaded(data) => data,
                MemEntryState::NotLoaded => continue,
            };

            match resource.res_type {
                // for f in (ls img_*.dat); convert -size 320x200+0 -depth 8 gray:$f $f.png; end
                ResType::Bitmap => {
                    let mut file =
                        File::create(format!("{}/img_{:02x}.dat", DUMPED_RESOURCES_DIR, i))?;
                    file.write_all(
                        &MemEntry::fixup_bitmap(data)
                            .iter()
                            .map(|x| x << 4)
                            .collect::<Vec<u8>>(),
                    )?;
                }
                ResType::Bytecode => {
                    let mut file =
                        File::create(format!("{}/code_{:02x}.dat", DUMPED_RESOURCES_DIR, i))
                            .unwrap();
                    file.write_all(data)?;
                }
                ResType::Cinematic => {
                    let mut file =
                        File::create(format!("{}/cine_{:02x}.dat", DUMPED_RESOURCES_DIR, i))
                            .unwrap();
                    file.write_all(data)?;
                }
                ResType::Sound => {
                    let mut file =
                        File::create(format!("{}/sound_{:02x}.dat", DUMPED_RESOURCES_DIR, i))
                            .unwrap();
                    file.write_all(data)?;
                }
                ResType::Music => {
                    let mut file =
                        File::create(format!("{}/music_{:02x}.dat", DUMPED_RESOURCES_DIR, i))
                            .unwrap();
                    file.write_all(data)?;
                }
                ResType::Palette => {
                    let mut file =
                        File::create(format!("{}/palette_{:02x}.dat", DUMPED_RESOURCES_DIR, i))
                            .unwrap();
                    file.write_all(data)?;
                }
                ResType::Unknown => {
                    let mut file =
                        File::create(format!("{}/unknown_{:02x}.dat", DUMPED_RESOURCES_DIR, i))
                            .unwrap();
                    file.write_all(data)?;
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
        let mut resman = ResourceManager::new()?;
        assert_ne!(resman.resources.len(), 0);

        for i in 1..resman.resources.len() {
            let expected_size = resman.resources[i].info.size;
            let resource = resman.load_resource(i)?;
            assert_eq!(expected_size, resource.data.len());
        }

        Ok(())
    }
}
