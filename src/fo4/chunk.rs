use crate::{
    containers::CompressableBytes,
    derive,
    fo4::{CompressionFormat, CompressionLevel, Error, Result},
};
use core::ops::Range;
use flate2::{
    write::{ZlibDecoder, ZlibEncoder},
    Compress, Compression,
};
use lzzzz::{lz4, lz4_hc};
use std::io::Write;

#[repr(transparent)]
pub struct OptionsBuilder(Options);

impl OptionsBuilder {
    #[must_use]
    pub fn build(self) -> Options {
        self.0
    }

    #[must_use]
    pub fn compression_format(mut self, compression_format: CompressionFormat) -> Self {
        self.0.compression_format = compression_format;
        self
    }

    #[must_use]
    pub fn compression_level(mut self, compression_level: CompressionLevel) -> Self {
        self.0.compression_level = compression_level;
        self
    }

    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for OptionsBuilder {
    fn default() -> Self {
        Self(Options {
            compression_format: CompressionFormat::default(),
            compression_level: CompressionLevel::default(),
        })
    }
}

#[derive(Clone, Copy)]
pub struct Options {
    compression_format: CompressionFormat,
    compression_level: CompressionLevel,
}

impl Options {
    #[must_use]
    pub fn builder() -> OptionsBuilder {
        OptionsBuilder::new()
    }

    #[must_use]
    pub fn compression_format(&self) -> CompressionFormat {
        self.compression_format
    }

    #[must_use]
    pub fn compression_level(&self) -> CompressionLevel {
        self.compression_level
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DX10 {
    pub mips: Range<u16>,
}

#[allow(clippy::upper_case_acronyms)]
#[non_exhaustive]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum Extra {
    #[default]
    GNRL,
    DX10(DX10),
}

impl From<DX10> for Extra {
    fn from(value: DX10) -> Self {
        Self::DX10(value)
    }
}

#[derive(Default)]
pub struct Chunk<'bytes> {
    pub(crate) bytes: CompressableBytes<'bytes>,
    pub extra: Extra,
}

derive::compressable_bytes!(Chunk);

impl<'bytes> Chunk<'bytes> {
    pub fn compress_into(&self, out: &mut Vec<u8>, options: &Options) -> Result<()> {
        if self.is_compressed() {
            Err(Error::AlreadyCompressed)
        } else {
            match options.compression_format {
                CompressionFormat::Zip => match options.compression_level {
                    CompressionLevel::FO4 => {
                        self.compress_into_zlib(out, Compression::default(), 15)
                    }
                    CompressionLevel::FO4Xbox => {
                        self.compress_into_zlib(out, Compression::best(), 12)
                    }
                    CompressionLevel::SF => self.compress_into_zlib(out, Compression::best(), 15),
                },
                CompressionFormat::LZ4 => self.compress_into_lz4(out),
            }
        }
    }

    pub fn decompress_into(&self, out: &mut Vec<u8>, options: &Options) -> Result<()> {
        let Some(decompressed_len) = self.decompressed_len() else {
            return Err(Error::AlreadyDecompressed);
        };

        out.reserve_exact(decompressed_len);
        let out_len = match options.compression_format {
            CompressionFormat::Zip => self.decompress_into_zlib(out),
            CompressionFormat::LZ4 => self.decompress_into_lz4(out),
        }?;

        if out_len == decompressed_len {
            Ok(())
        } else {
            Err(Error::DecompressionSizeMismatch {
                expected: decompressed_len,
                actual: out_len,
            })
        }
    }

    pub(crate) fn from_bytes(bytes: CompressableBytes<'_>) -> Chunk<'_> {
        Chunk {
            bytes,
            extra: Extra::default(),
        }
    }

    fn compress_into_lz4(&self, out: &mut Vec<u8>) -> Result<()> {
        lz4_hc::compress_to_vec(self.as_bytes(), out, lz4_hc::CLEVEL_MAX)?;
        Ok(())
    }

    fn compress_into_zlib(
        &self,
        out: &mut Vec<u8>,
        level: Compression,
        window_bits: u8,
    ) -> Result<()> {
        let mut e = ZlibEncoder::new_with_compress(
            out,
            Compress::new_with_window_bits(level, true, window_bits),
        );
        e.write_all(self.as_bytes())?;
        e.finish()?;
        Ok(())
    }

    fn decompress_into_lz4(&self, out: &mut [u8]) -> Result<usize> {
        let len = lz4::decompress(self.as_bytes(), out)?;
        Ok(len)
    }

    fn decompress_into_zlib(&self, out: &mut Vec<u8>) -> Result<usize> {
        let mut d = ZlibDecoder::new(out);
        d.write_all(self.as_bytes())?;
        Ok(d.total_out().try_into()?)
    }
}

#[cfg(test)]
mod tests {
    use super::{Chunk, Extra};

    #[test]
    fn default_state() {
        let c = Chunk::default();
        assert!(c.is_empty());
        assert!(!c.is_compressed());
        assert!(c.is_decompressed());
        assert_eq!(c.len(), 0);
        assert_eq!(c.extra, Extra::GNRL);
    }
}
