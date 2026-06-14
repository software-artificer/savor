use std::{fmt, io, time};

/// The error type for all MP4 parsing operations.
#[derive(thiserror::Error, Debug)]
pub enum ParseError {
    /// Returned when the input stream fails to provide the current cursor position.
    #[error("Failed to read current stream position")]
    CurrentStreamPosition(#[source] io::Error),

    /// Returned when an atom has an invalid size field: either a standard (2–7 bytes) or an
    /// extended (under 16 bytes).
    #[error("Atom size must be at least {0} bytes, got: {0}")]
    AtomSize(u64, u64),

    /// Returned when an input stream seek operation fails.
    #[error("Failed to seek the stream")]
    Seek(#[source] io::Error),

    /// Returned when reading from the input stream fails.
    #[error("Failed to read the data from the stream")]
    Read(#[source] io::Error),

    /// Returned when an atom has an unsupported version field.
    #[error("Unsupported atom version {0} for {1}")]
    AtomVersion(u8, AtomType),

    /// Returned when a child atom's bounds exceed those of its parent.
    #[error("The size of {0} atom extends beyond its parent")]
    SizeOverlap(AtomType),

    /// Returned when a required child atom of a specific type is missing.
    #[error("Unable to find a {0} atom")]
    MissingAtom(AtomType),

    /// Returned when multiple atoms of a specific type are found where only one is expected.
    #[error("Found duplicate atom type {0}")]
    DuplicateAtom(AtomType),

    /// Returned on arithmetic overflow or division by zero during atom property calculations.
    #[error("Overflow occurred while calculating offset for atom {0}")]
    MathError(AtomType),

    /// Returned when the time-to-sample (stts) table is empty, possibly indicating a fragmented
    /// stream.
    #[error("The stts atom contained 0 samples, possibly a fragmented file")]
    Fragmented,
}

#[derive(Copy, Clone, Debug)]
enum Size {
    Standard(u32),
    Extended(u64),
    EndOfStream(u64),
}

/// Represents an atom's size, covering standard, extended, or end-of-stream values.
#[derive(Copy, Clone, Debug)]
pub struct AtomSize(Size);

impl AtomSize {
    fn header_offset(&self) -> u64 {
        match self.0 {
            Size::Extended(_) => 16,
            _ => 8,
        }
    }

    fn content_size(&self) -> u64 {
        u64::from(*self) - self.header_offset()
    }
}

impl From<AtomSize> for u64 {
    fn from(value: AtomSize) -> Self {
        match value.0 {
            Size::Standard(size) => size as u64,
            Size::Extended(size) | Size::EndOfStream(size) => size,
        }
    }
}

/// Returned when an atom's size field is invalid or requires additional processing.
#[derive(Debug)]
pub enum SizeError {
    /// Internal variant used when the atom size must be read from the extended size field.
    /// This is handled by the parser and never returned to the caller.
    Extended,
    /// Indicates the atom's size field value is too small.
    ///
    /// - A standard 32-bit size field must be at least 8 bytes (4 for size, 4 for type).
    /// - An extended 64-bit size field must be at least 16 bytes (4 for size set to 1, 4 for type,
    ///   and 8 for the extended field).
    TooSmall(u64),
    /// Internal variant used for atoms spanning until the end of the stream.
    /// This is handled by the parser and never returned to the caller.
    EndOfStream,
}

impl TryFrom<u32> for AtomSize {
    type Error = SizeError;

    fn try_from(size: u32) -> Result<Self, Self::Error> {
        match size {
            0 => Err(Self::Error::EndOfStream),
            1 => Err(Self::Error::Extended),
            2..8 => Err(Self::Error::TooSmall(size as u64)),
            size => Ok(Self(Size::Standard(size))),
        }
    }
}

impl TryFrom<u64> for AtomSize {
    type Error = u64;

    fn try_from(size: u64) -> Result<Self, u64> {
        match size {
            0..16 => Err(size),
            size => Ok(Self(Size::Extended(size))),
        }
    }
}

impl fmt::Display for AtomSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self(Size::Standard(s)) => write!(f, "{s}"),
            Self(Size::Extended(s)) | Self(Size::EndOfStream(s)) => {
                write!(f, "{s}")
            }
        }
    }
}

/// Represents the four-byte identifier of an atom type.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum AtomType {
    /// Represents atoms of types not explicitly handled by the parser.
    Other([u8; 4]),
    /// Represents the `moov` (movie atom) box.
    Moov,
    /// Represents the `mvhd` (movie header atom) box.
    Mvhd,
    /// Represents the `trak` (track atom) box.
    Trak,
    /// Represents the `udta` (user data atom) box.
    Udta,
    /// Represents the `mdia` (media atom) box.
    Mdia,
    /// Represents the `tkhd` (track header atom) box.
    Tkhd,
    /// Represents the `hdlr` (media handler type atom) box.
    Hdlr,
    /// Represents the `minf` (media information atom) box.
    Minf,
    /// Represents the `stbl` (sample table atom) box.
    Stbl,
    /// Represents the `stts` (time-to-sample atom) box.
    Stts,
    /// Represents the `mdhd` (media header atom) box.
    Mdhd,
}

impl fmt::Display for AtomType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Moov => write!(f, "moov"),
            Self::Mvhd => write!(f, "mvhd"),
            Self::Trak => write!(f, "trak"),
            Self::Udta => write!(f, "udta"),
            Self::Mdia => write!(f, "mdia"),
            Self::Tkhd => write!(f, "tkhd"),
            Self::Hdlr => write!(f, "hdlr"),
            Self::Minf => write!(f, "minf"),
            Self::Stbl => write!(f, "stbl"),
            Self::Stts => write!(f, "stts"),
            Self::Mdhd => write!(f, "mdhd"),
            Self::Other(t) => match str::from_utf8(t) {
                Ok(t) if t.chars().all(char::is_alphanumeric) => write!(f, "{t}"),
                _ => write!(f, "[{:02x} {:02x} {:02x} {:02x}]", t[0], t[1], t[2], t[3]),
            },
        }
    }
}

impl From<[u8; 4]> for AtomType {
    fn from(value: [u8; 4]) -> Self {
        match &value {
            b"moov" => Self::Moov,
            b"mvhd" => Self::Mvhd,
            b"trak" => Self::Trak,
            b"udta" => Self::Udta,
            b"mdia" => Self::Mdia,
            b"tkhd" => Self::Tkhd,
            b"hdlr" => Self::Hdlr,
            b"minf" => Self::Minf,
            b"stbl" => Self::Stbl,
            b"stts" => Self::Stts,
            b"mdhd" => Self::Mdhd,
            _ => Self::Other(value),
        }
    }
}

#[derive(Copy, Clone)]
#[cfg_attr(test, derive(Debug))]
struct AtomBounds {
    position: u64,
    size: AtomSize,
}

impl AtomBounds {
    fn ends_at(&self) -> Option<u64> {
        self.position.checked_add(u64::from(self.size))
    }

    fn content_size(&self) -> u64 {
        self.size.content_size()
    }

    fn content_position(&self) -> Option<u64> {
        self.position.checked_add(self.size.header_offset())
    }
}

/// Represents an atom header's type and size alongside its recorded stream position.
///
/// The size encompasses standard, extended, and end-of-stream variants.
#[derive(Copy, Clone)]
#[cfg_attr(test, derive(Debug))]
pub struct AtomHeader {
    bounds: AtomBounds,
    atom_type: AtomType,
}

impl AtomHeader {
    fn parse_from_stream<T: io::Read + io::Seek>(stream: &mut T) -> Result<Self, ParseError> {
        let mut hdr_buf = [0u8; 8];

        let position = stream
            .stream_position()
            .map_err(ParseError::CurrentStreamPosition)?;

        stream.read_exact(&mut hdr_buf).map_err(ParseError::Read)?;

        let size = u32::from_be_bytes([hdr_buf[0], hdr_buf[1], hdr_buf[2], hdr_buf[3]]);

        let size = match AtomSize::try_from(size) {
            Ok(size) => Ok(size),
            Err(SizeError::Extended) => {
                let mut buf = [0u8; 8];

                stream.read_exact(&mut buf).map_err(ParseError::Read)?;

                match AtomSize::try_from(u64::from_be_bytes(buf)) {
                    Ok(size) => Ok(size),
                    Err(size) => Err(ParseError::AtomSize(16, size)),
                }
            }
            Err(SizeError::TooSmall(s)) => Err(ParseError::AtomSize(8, s)),
            Err(SizeError::EndOfStream) => Ok(AtomSize(Size::EndOfStream(get_stream_end(stream)?))),
        }?;

        let atom_type = AtomType::from([hdr_buf[4], hdr_buf[5], hdr_buf[6], hdr_buf[7]]);

        let bounds = AtomBounds { position, size };

        Ok(AtomHeader { bounds, atom_type })
    }

    fn parse_container<S, F, R>(
        &self,
        mut result: R,
        stream: &mut S,
        mut callback: F,
    ) -> Result<R, ParseError>
    where
        S: io::Read + io::Seek,
        F: FnMut(R, AtomHeader, &mut S) -> Result<R, ParseError>,
    {
        let pos = self.content_position()?;
        let end = self.ends_at()?;

        stream
            .seek(io::SeekFrom::Start(pos))
            .map_err(ParseError::Seek)?;

        while stream
            .stream_position()
            .map_err(ParseError::CurrentStreamPosition)?
            < end
        {
            let atom = AtomHeader::parse_from_stream(stream)?;

            if atom.ends_at()? > end {
                return Err(ParseError::SizeOverlap(atom.atom_type));
            }

            result = callback(result, atom, stream)?;

            atom.skip(stream)?;
        }

        Ok(result)
    }

    /// Finds a child atom of the specified type within the current atom's boundaries by parsing
    /// the provided stream.
    ///
    /// # Errors
    /// - Returns [ParseError] if parsing fails.
    /// - Returns [ParseError::DuplicateAtom] if more than one child of the requested type is
    ///   found.
    pub fn find_child<S: io::Read + io::Seek>(
        &self,
        stream: &mut S,
        atom_type: AtomType,
    ) -> Result<Option<Self>, ParseError> {
        self.parse_container(None, stream, |atom, header, _| {
            if header.atom_type == atom_type {
                if atom.is_some() {
                    Err(ParseError::DuplicateAtom(atom_type))
                } else {
                    Ok(Some(header))
                }
            } else {
                Ok(atom)
            }
        })
    }

    /// Finds all child atoms of the specified type within the current atom's boundaries by parsing
    /// the provided stream.
    ///
    /// Returns an empty vector if no matching atoms are found.
    ///
    /// # Errors
    /// - Returns [ParseError] if parsing fails.
    pub fn find_children<S: io::Read + io::Seek>(
        &self,
        stream: &mut S,
        atom_type: AtomType,
    ) -> Result<Vec<AtomHeader>, ParseError> {
        self.parse_container(vec![], stream, |mut atoms, header, _| {
            if header.atom_type == atom_type {
                atoms.push(header)
            }

            Ok(atoms)
        })
    }

    fn skip<S: io::Read + io::Seek>(&self, stream: &mut S) -> Result<u64, ParseError> {
        let pos = self.ends_at()?;

        stream
            .seek(io::SeekFrom::Start(pos))
            .map_err(ParseError::Seek)
    }

    fn ends_at(&self) -> Result<u64, ParseError> {
        self.bounds
            .ends_at()
            .ok_or(ParseError::MathError(self.atom_type))
    }

    /// Returns the atom's content size, excluding the size field, type identifier, and extended
    /// size field (if present).
    pub fn content_size(&self) -> u64 {
        self.bounds.content_size()
    }

    /// Returns the atom's content poisition, excluding the size field, type identifier, and
    /// extended size field (if present).
    pub fn content_position(&self) -> Result<u64, ParseError> {
        self.bounds
            .content_position()
            .ok_or(ParseError::MathError(self.atom_type))
    }

    /// Returns the recorded position of the atom within the stream.
    pub fn position(&self) -> u64 {
        self.bounds.position
    }

    /// Returns the total size of the atom, including the header.
    pub fn size(&self) -> AtomSize {
        self.bounds.size
    }
}

fn get_stream_end<S: io::Read + io::Seek>(stream: &mut S) -> Result<u64, ParseError> {
    let pos = stream
        .stream_position()
        .map_err(ParseError::CurrentStreamPosition)?;

    let end = stream
        .seek(io::SeekFrom::End(0))
        .map_err(ParseError::Seek)?;

    stream
        .seek(io::SeekFrom::Start(pos))
        .map_err(ParseError::Seek)?;

    Ok(end)
}

#[cfg_attr(test, derive(Debug))]
enum MvhdAtom {
    V0 {
        _bounds: AtomBounds,
        timescale: u32,
        duration: u32,
    },
    V1 {
        _bounds: AtomBounds,
        timescale: u32,
        duration: u64,
    },
}

impl MvhdAtom {
    fn parse_from_stream<S: io::Read + io::Seek>(
        bounds: AtomBounds,
        stream: &mut S,
    ) -> Result<Self, ParseError> {
        let mut mvhd_hdr = [0u8; 4];

        stream.read_exact(&mut mvhd_hdr).map_err(ParseError::Read)?;

        match mvhd_hdr[0] {
            0 => {
                let mut mvhd_buf = [0u8; 16];

                stream.read_exact(&mut mvhd_buf).map_err(ParseError::Read)?;

                let timescale =
                    u32::from_be_bytes([mvhd_buf[8], mvhd_buf[9], mvhd_buf[10], mvhd_buf[11]]);

                let duration =
                    u32::from_be_bytes([mvhd_buf[12], mvhd_buf[13], mvhd_buf[14], mvhd_buf[15]]);

                Ok(Self::V0 {
                    _bounds: bounds,
                    timescale,
                    duration,
                })
            }
            1 => {
                let mut mvhd_buf = [0u8; 28];

                stream.read_exact(&mut mvhd_buf).map_err(ParseError::Read)?;

                let timescale =
                    u32::from_be_bytes([mvhd_buf[16], mvhd_buf[17], mvhd_buf[18], mvhd_buf[19]]);

                let duration = u64::from_be_bytes([
                    mvhd_buf[20],
                    mvhd_buf[21],
                    mvhd_buf[22],
                    mvhd_buf[23],
                    mvhd_buf[24],
                    mvhd_buf[25],
                    mvhd_buf[26],
                    mvhd_buf[27],
                ]);

                Ok(Self::V1 {
                    _bounds: bounds,
                    timescale,
                    duration,
                })
            }
            v => Err(ParseError::AtomVersion(v, AtomType::Mvhd)),
        }
    }

    fn duration(&self) -> time::Duration {
        match self {
            Self::V0 {
                timescale,
                duration,
                ..
            } => time::Duration::from_secs((duration / timescale) as u64),
            Self::V1 {
                timescale,
                duration,
                ..
            } => time::Duration::from_secs(duration / *timescale as u64),
        }
    }
}

#[cfg_attr(test, derive(Debug))]
struct TkhdAtom {
    _bounds: AtomBounds,
    width: u32,
    height: u32,
}

impl TkhdAtom {
    fn parse_from_stream<S: io::Read + io::Seek>(
        bounds: AtomBounds,
        stream: &mut S,
    ) -> Result<Self, ParseError> {
        let mut tkhd_buf = [0u8; 4];

        stream.read_exact(&mut tkhd_buf).map_err(ParseError::Read)?;

        let offset = match tkhd_buf[0] {
            0 => Ok(72),
            1 => Ok(84),
            v => Err(ParseError::AtomVersion(v, AtomType::Tkhd)),
        }?;

        stream
            .seek(io::SeekFrom::Current(offset))
            .map_err(ParseError::Seek)?;

        let mut res_buf = [0u8; 8];

        stream.read_exact(&mut res_buf).map_err(ParseError::Read)?;

        let width = u32::from_be_bytes([res_buf[0], res_buf[1], res_buf[2], res_buf[3]]) >> 16;
        let height = u32::from_be_bytes([res_buf[4], res_buf[5], res_buf[6], res_buf[7]]) >> 16;

        Ok(TkhdAtom {
            _bounds: bounds,
            width,
            height,
        })
    }

    fn resolution(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

#[derive(Default)]
#[cfg_attr(test, derive(Debug))]
struct MoovAtomBuilder {
    mvhd: Option<MvhdAtom>,
    trak: Option<TrakVideAtom>,
    udta: Option<AtomBounds>,
}

impl MoovAtomBuilder {
    fn build<T>(self, bounds: AtomBounds, stream: T) -> Result<MoovAtom<T>, ParseError>
    where
        T: io::Read + io::Seek,
    {
        let mvhd = self.mvhd.ok_or(ParseError::MissingAtom(AtomType::Mvhd))?;

        let trak = self.trak.ok_or(ParseError::MissingAtom(AtomType::Trak))?;

        let udta = self.udta;

        Ok(MoovAtom {
            _bounds: bounds,
            trak,
            mvhd,
            _udta: udta,
            _stream: stream,
        })
    }

    fn udta(self, udta: AtomBounds) -> Result<Self, ParseError> {
        if self.udta.is_some() {
            Err(ParseError::DuplicateAtom(AtomType::Udta))
        } else {
            Ok(Self {
                udta: Some(udta),
                ..self
            })
        }
    }

    fn trak(self, trak: TrakAtom) -> Result<Self, ParseError> {
        match trak {
            TrakAtom::Vide(_) if self.trak.is_some() => {
                Err(ParseError::DuplicateAtom(AtomType::Trak))
            }
            TrakAtom::Vide(trak) => Ok(Self {
                trak: Some(trak),
                ..self
            }),
            TrakAtom::Other(_) => Ok(self),
        }
    }

    fn mvhd(self, mvhd: MvhdAtom) -> Result<Self, ParseError> {
        if self.mvhd.is_some() {
            Err(ParseError::DuplicateAtom(AtomType::Mvhd))
        } else {
            Ok(Self {
                mvhd: Some(mvhd),
                ..self
            })
        }
    }
}

/// Represents the `moov` (movie atom) box, a container for all metadata.
pub struct MoovAtom<S> {
    _bounds: AtomBounds,
    mvhd: MvhdAtom,
    trak: TrakVideAtom,
    _udta: Option<AtomBounds>,
    _stream: S,
}

impl<S> MoovAtom<S>
where
    S: io::Read + io::Seek,
{
    /// Returns the media duration.
    pub fn duration(&self) -> time::Duration {
        self.mvhd.duration()
    }

    /// Returns the video resolution in pixels.
    pub fn resolution(&self) -> (u32, u32) {
        self.trak.resolution()
    }

    fn parse_from_stream(header: AtomHeader, mut stream: S) -> Result<Self, ParseError> {
        header
            .parse_container(
                Self::builder(),
                &mut stream,
                |builder, atom, stream| match atom.atom_type {
                    AtomType::Mvhd => {
                        let mvhd = MvhdAtom::parse_from_stream(atom.bounds, stream)?;

                        builder.mvhd(mvhd)
                    }
                    AtomType::Trak => {
                        let trak = TrakAtom::parse_from_stream(atom, stream)?;

                        builder.trak(trak)
                    }
                    AtomType::Udta => builder.udta(atom.bounds),
                    _ => {
                        tracing::debug!(
                            atom_type = ?atom.atom_type,
                            atom_position = atom.position(),
                            atom_size = ?atom.size(),
                            "Ignoring irrelevant atom",
                        );

                        Ok(builder)
                    }
                },
            )?
            .build(header.bounds, stream)
    }

    fn builder() -> MoovAtomBuilder {
        MoovAtomBuilder::default()
    }

    /// Returns the calculated average FPS (frames per second).
    pub fn fps(&self) -> Result<u64, ParseError> {
        self.trak.fps()
    }
}

struct AtomBorrow<'a, S>(&'a mut S);

impl<S> AtomBorrow<'_, S>
where
    S: io::Read,
{
    fn read(&mut self) -> io::Result<std::io::Bytes> {
        self.
    }
}

/*
fn play() {
    let stream = std::io::Cursor::new(vec![]);
    let moov = parse(stream).unwrap();

    let atoms = moov
        // Atom
        .udta()
        // Iter
        .get_atoms(AtomType::Other(b"savo"));

    let atom = atoms
        // Option<Atom>
        .next();

    let contents = atom.read();

    atom.replace(contents);

    let atom = atoms.next();

    atom.get_atoms();

    atoms.append(other_contents);
    atoms.insert(yet_another_one);

    let atom = atoms.next();
    atom.remove();
}
*/

#[cfg_attr(test, derive(Debug))]
struct TrakVideAtom {
    _bounds: AtomBounds,
    mdia: MdiaAtom,
    tkhd: TkhdAtom,
}

impl TrakVideAtom {
    fn fps(&self) -> Result<u64, ParseError> {
        self.mdia.fps()
    }

    fn resolution(&self) -> (u32, u32) {
        self.tkhd.resolution()
    }
}

#[derive(Default)]
#[cfg_attr(test, derive(Debug))]
struct TrakAtomBuilder {
    mdia: Option<MdiaAtom>,
    tkhd: Option<TkhdAtom>,
}

impl TrakAtomBuilder {
    fn build(self, header: AtomHeader) -> Result<TrakAtom, ParseError> {
        let tkhd = self.tkhd.ok_or(ParseError::MissingAtom(AtomType::Tkhd))?;

        let mdia = self.mdia.ok_or(ParseError::MissingAtom(AtomType::Mdia))?;

        match mdia.hdlr {
            HdlrAtom::Vide(_) => Ok(TrakAtom::Vide(TrakVideAtom {
                _bounds: header.bounds,
                tkhd,
                mdia,
            })),
            HdlrAtom::Other(_) => Ok(TrakAtom::Other(header.bounds)),
        }
    }

    fn tkhd(self, tkhd: TkhdAtom) -> Result<Self, ParseError> {
        if self.tkhd.is_some() {
            Err(ParseError::DuplicateAtom(AtomType::Tkhd))
        } else {
            Ok(Self {
                tkhd: Some(tkhd),
                ..self
            })
        }
    }

    fn mdia(self, mdia: MdiaAtom) -> Result<Self, ParseError> {
        if self.mdia.is_some() {
            Err(ParseError::DuplicateAtom(AtomType::Mdia))
        } else {
            Ok(Self {
                mdia: Some(mdia),
                ..self
            })
        }
    }
}

#[cfg_attr(test, derive(Debug))]
#[allow(clippy::large_enum_variant)]
enum TrakAtom {
    Vide(TrakVideAtom),
    #[allow(dead_code)]
    Other(AtomBounds),
}

impl TrakAtom {
    fn builder() -> TrakAtomBuilder {
        TrakAtomBuilder::default()
    }
}

impl TrakAtom {
    fn parse_from_stream<S: io::Read + io::Seek>(
        header: AtomHeader,
        stream: &mut S,
    ) -> Result<Self, ParseError> {
        header
            .parse_container(
                Self::builder(),
                stream,
                |builder, atom, stream| match atom.atom_type {
                    AtomType::Mdia => {
                        let mdia = MdiaAtom::parse_from_stream(atom, stream)?;

                        builder.mdia(mdia)
                    }
                    AtomType::Tkhd => {
                        let tkhd = TkhdAtom::parse_from_stream(atom.bounds, stream)?;

                        builder.tkhd(tkhd)
                    }
                    _ => {
                        tracing::debug!(
                            atom_type = ?atom.atom_type,
                            atom_position = atom.position(),
                            atom_size = ?atom.size(),
                            "Ignoring irrelevant atom",
                        );

                        Ok(builder)
                    }
                },
            )?
            .build(header)
    }
}

#[cfg_attr(test, derive(Debug))]
struct SttsAtom {
    _bounds: AtomBounds,
    table: Vec<(u32, u32)>,
}

impl SttsAtom {
    fn parse_from_stream<S: io::Read + io::Seek>(
        bounds: AtomBounds,
        stream: &mut S,
    ) -> Result<Self, ParseError> {
        let mut buf = [0u8; 4];

        stream.read_exact(&mut buf).map_err(ParseError::Read)?;

        match buf[0] {
            0 => {
                stream.read_exact(&mut buf).map_err(ParseError::Read)?;

                let count = u32::from_be_bytes(buf);

                if count == 0 {
                    return Err(ParseError::Fragmented);
                }

                let mut buf = [0u8; 8];

                let mut table = vec![];

                for _ in 0..count {
                    stream.read_exact(&mut buf).map_err(ParseError::Read)?;

                    let sample_count = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
                    let sample_delta = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);

                    table.push((sample_count, sample_delta));
                }

                Ok(Self {
                    _bounds: bounds,
                    table,
                })
            }
            v => Err(ParseError::AtomVersion(v, AtomType::Stts)),
        }
    }

    fn frame_count(&self) -> Result<u64, ParseError> {
        self.table
            .iter()
            .map(|(count, _)| *count as u64)
            .try_fold(0u64, |sum, c| sum.checked_add(c))
            .ok_or(ParseError::MathError(AtomType::Stts))
    }

    fn total_time_units(&self) -> Result<u64, ParseError> {
        self.table
            .iter()
            .map(|(c, s)| (*c as u64).checked_mul(*s as u64))
            .try_fold(0u64, |sum, tu| tu.and_then(|tu| tu.checked_add(sum)))
            .ok_or(ParseError::MathError(AtomType::Stts))
    }
}

#[cfg_attr(test, derive(Debug))]
struct StblAtom {
    _bounds: AtomBounds,
    stts: SttsAtom,
}

impl StblAtom {
    fn parse_from_stream<S: io::Read + io::Seek>(
        header: AtomHeader,
        stream: &mut S,
    ) -> Result<Self, ParseError> {
        let stts = header
            .parse_container(None, stream, |stts, atom, stream| match atom.atom_type {
                AtomType::Stts if stts.is_some() => Err(ParseError::DuplicateAtom(AtomType::Stts)),
                AtomType::Stts => {
                    let stts = SttsAtom::parse_from_stream(atom.bounds, stream)?;

                    Ok(Some(stts))
                }
                _ => {
                    tracing::debug!(
                        atom_type = ?atom.atom_type,
                        atom_position = atom.position(),
                        atom_size = ?atom.size(),
                        "Ignoring irrelevant atom",
                    );

                    Ok(stts)
                }
            })?
            .ok_or(ParseError::MissingAtom(AtomType::Stts))?;

        Ok(Self {
            _bounds: header.bounds,
            stts,
        })
    }

    fn frame_count(&self) -> Result<u64, ParseError> {
        self.stts.frame_count()
    }

    fn total_time_units(&self) -> Result<u64, ParseError> {
        self.stts.total_time_units()
    }
}

#[cfg_attr(test, derive(Debug))]
struct MinfAtom {
    _bounds: AtomBounds,
    stbl: StblAtom,
}

impl MinfAtom {
    fn parse_from_stream<S: io::Read + io::Seek>(
        header: AtomHeader,
        stream: &mut S,
    ) -> Result<Self, ParseError> {
        let stbl = header
            .parse_container(None, stream, |stbl, atom, stream| match atom.atom_type {
                AtomType::Stbl if stbl.is_some() => Err(ParseError::DuplicateAtom(AtomType::Stbl)),
                AtomType::Stbl => {
                    let stbl_atom = StblAtom::parse_from_stream(atom, stream)?;

                    Ok(Some(stbl_atom))
                }
                _ => {
                    tracing::debug!(
                        atom_type = ?atom.atom_type,
                        atom_position = atom.position(),
                        atom_size = ?atom.size(),
                        "Ignoring irrelevant atom",
                    );

                    Ok(stbl)
                }
            })?
            .ok_or(ParseError::MissingAtom(AtomType::Stbl))?;

        Ok(MinfAtom {
            _bounds: header.bounds,
            stbl,
        })
    }

    fn total_time_units(&self) -> Result<u64, ParseError> {
        self.stbl.total_time_units()
    }

    fn frame_count(&self) -> Result<u64, ParseError> {
        self.stbl.frame_count()
    }
}

#[cfg_attr(test, derive(Debug))]
struct MdhdAtom {
    _bounds: AtomBounds,
    timescale: u32,
}

impl MdhdAtom {
    fn parse_from_stream<S: io::Read + io::Seek>(
        header: AtomHeader,
        stream: &mut S,
    ) -> Result<Self, ParseError> {
        let mut hdr_buf = [0u8; 4];

        stream.read_exact(&mut hdr_buf).map_err(ParseError::Read)?;

        let timescale = match hdr_buf[0] {
            0 => {
                let mut buf = [0u8; 12];

                stream.read_exact(&mut buf).map_err(ParseError::Read)?;

                Ok(u32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]))
            }
            1 => {
                let mut buf = [0u8; 20];

                stream.read_exact(&mut buf).map_err(ParseError::Read)?;

                Ok(u32::from_be_bytes([buf[16], buf[17], buf[18], buf[19]]))
            }
            v => Err(ParseError::AtomVersion(v, AtomType::Mdhd)),
        }?;

        Ok(Self {
            _bounds: header.bounds,
            timescale,
        })
    }

    fn timescale(&self) -> u64 {
        self.timescale as u64
    }
}

#[cfg_attr(test, derive(Debug))]
struct MdiaAtom {
    _bounds: AtomBounds,
    hdlr: HdlrAtom,
    minf: MinfAtom,
    mdhd: MdhdAtom,
}

impl MdiaAtom {
    fn builder() -> MdiaAtomBuilder {
        MdiaAtomBuilder::default()
    }

    fn parse_from_stream<S: io::Read + io::Seek>(
        header: AtomHeader,
        stream: &mut S,
    ) -> Result<Self, ParseError> {
        header
            .parse_container(
                Self::builder(),
                stream,
                |builder, atom, stream| match atom.atom_type {
                    AtomType::Hdlr => {
                        let hdlr = HdlrAtom::parse_from_stream(atom, stream)?;

                        builder.hdlr(hdlr)
                    }
                    AtomType::Minf => {
                        let minf = MinfAtom::parse_from_stream(atom, stream)?;

                        builder.minf(minf)
                    }
                    AtomType::Mdhd => {
                        let mdhd = MdhdAtom::parse_from_stream(atom, stream)?;

                        builder.mdhd(mdhd)
                    }
                    _ => {
                        tracing::debug!(
                            atom_type = ?atom.atom_type,
                            atom_position = atom.position(),
                            atom_size = ?atom.size(),
                            "Ignoring irrelevant atom",
                        );

                        Ok(builder)
                    }
                },
            )?
            .build(header.bounds)
    }

    fn fps(&self) -> Result<u64, ParseError> {
        self.minf
            .frame_count()
            .and_then(|fc| {
                fc.checked_mul(self.mdhd.timescale())
                    .ok_or(ParseError::MathError(AtomType::Mdia))
            })
            .and_then(|res| {
                res.checked_div(self.minf.total_time_units()?)
                    .ok_or(ParseError::MathError(AtomType::Mdia))
            })
    }
}

#[derive(Default)]
#[cfg_attr(test, derive(Debug))]
struct MdiaAtomBuilder {
    hdlr: Option<HdlrAtom>,
    minf: Option<MinfAtom>,
    mdhd: Option<MdhdAtom>,
}

impl MdiaAtomBuilder {
    fn build(self, bounds: AtomBounds) -> Result<MdiaAtom, ParseError> {
        let hdlr = self.hdlr.ok_or(ParseError::MissingAtom(AtomType::Hdlr))?;

        let minf = self.minf.ok_or(ParseError::MissingAtom(AtomType::Minf))?;

        let mdhd = self.mdhd.ok_or(ParseError::MissingAtom(AtomType::Mdhd))?;

        Ok(MdiaAtom {
            _bounds: bounds,
            hdlr,
            minf,
            mdhd,
        })
    }

    fn hdlr(self, hdlr: HdlrAtom) -> Result<Self, ParseError> {
        if self.hdlr.is_some() {
            Err(ParseError::DuplicateAtom(AtomType::Hdlr))
        } else {
            Ok(Self {
                hdlr: Some(hdlr),
                ..self
            })
        }
    }

    fn minf(self, minf: MinfAtom) -> Result<MdiaAtomBuilder, ParseError> {
        if self.minf.is_some() {
            Err(ParseError::DuplicateAtom(AtomType::Minf))
        } else {
            Ok(Self {
                minf: Some(minf),
                ..self
            })
        }
    }

    fn mdhd(self, mdhd: MdhdAtom) -> Result<MdiaAtomBuilder, ParseError> {
        if self.mdhd.is_some() {
            Err(ParseError::DuplicateAtom(AtomType::Mdhd))
        } else {
            Ok(Self {
                mdhd: Some(mdhd),
                ..self
            })
        }
    }
}

#[cfg_attr(test, derive(Debug))]
enum HdlrAtom {
    #[allow(dead_code)]
    Vide(AtomBounds),
    #[allow(dead_code)]
    Other(AtomBounds),
}

impl HdlrAtom {
    fn parse_from_stream<S: io::Read + io::Seek>(
        header: AtomHeader,
        stream: &mut S,
    ) -> Result<Self, ParseError> {
        let mut hdr_buf = [0u8; 4];

        stream.read_exact(&mut hdr_buf).map_err(ParseError::Read)?;

        let version = hdr_buf[0];

        if version != 0 {
            return Err(ParseError::AtomVersion(version, AtomType::Hdlr));
        }

        let mut buf = [0u8; 8];

        stream.read_exact(&mut buf).map_err(ParseError::Read)?;

        match &buf[4..] {
            b"vide" => Ok(Self::Vide(header.bounds)),
            _ => Ok(Self::Other(header.bounds)),
        }
    }
}

/// Parses the stream to extract the `moov` (movie atom) box and its descendant atoms.
///
/// This is the primary entry point for the parser.
///
/// # Errors
/// - Returns [ParseError] if parsing fails.
pub fn parse<T: io::Read + io::Seek>(mut stream: T) -> Result<MoovAtom<T>, ParseError> {
    let moov_header = loop {
        let atom = AtomHeader::parse_from_stream(&mut stream)?;

        if let AtomType::Moov = atom.atom_type {
            break atom;
        }

        tracing::debug!(
            atom_type = ?atom.atom_type,
            atom_position = atom.position(),
            atom_size = ?atom.size(),
            "Ignoring irrelevant atom",
        );

        atom.skip(&mut stream)?;
    };

    MoovAtom::parse_from_stream(moov_header, stream)
}

#[cfg(test)]
mod tests {
    use super::{
        AtomBounds, AtomHeader, AtomSize, AtomType, HdlrAtom, MdhdAtom, MdiaAtom, MinfAtom,
        MvhdAtom, Size, StblAtom, SttsAtom, TkhdAtom, TrakVideAtom,
    };
    use std::{
        any, fmt,
        io::{self, Read as _, Seek, Write as _},
    };

    #[cfg(test)]
    impl<S> fmt::Debug for super::MoovAtom<S> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("MoovAtom")
                .field("_bounds", &self._bounds)
                .field("mvhd", &self.mvhd)
                .field("trak", &self.trak)
                .field("udta", &self._udta)
                .field("_stream", &any::type_name::<S>())
                .finish()
        }
    }

    mockall::mock! {
        #[derive(Debug)]
        Stream {}

        impl io::Read for Stream {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>;
        }

        impl io::Seek for Stream {
            fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64>;
        }
    }

    struct TestStream<OnRead, S>
    where
        S: io::Seek,
        OnRead: Fn(&mut S) -> io::Result<()>,
    {
        stream: S,
        on_read: OnRead,
    }

    impl<OnRead, S> io::Read for TestStream<OnRead, S>
    where
        S: io::Seek + io::Read,
        OnRead: Fn(&mut S) -> io::Result<()>,
    {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            (self.on_read)(&mut self.stream)?;

            self.stream.read(buf)
        }
    }

    impl<OnRead, S> io::Seek for TestStream<OnRead, S>
    where
        S: io::Seek,
        OnRead: Fn(&mut S) -> io::Result<()>,
    {
        fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
            self.stream.seek(pos)
        }
    }

    fn get_mock_tkhd_atom(position: u64) -> TkhdAtom {
        TkhdAtom {
            _bounds: AtomBounds {
                position,
                size: AtomSize(Size::Standard(92)),
            },
            width: 1920,
            height: 1080,
        }
    }

    fn get_mock_stts_atom(position: u64) -> SttsAtom {
        SttsAtom {
            _bounds: AtomBounds {
                position,
                size: AtomSize(Size::Standard(96)),
            },
            table: vec![
                (45, 31),
                (70, 27),
                (25, 17),
                (15, 30),
                (40, 64),
                (30, 51),
                (25, 23),
                (25, 28),
                (90, 24),
                (85, 39),
            ],
        }
    }

    fn get_mock_stbl_atom(position: u64) -> StblAtom {
        let stts = get_mock_stts_atom(position + 8);

        StblAtom {
            _bounds: AtomBounds {
                position,
                size: AtomSize(Size::Standard(104)),
            },
            stts,
        }
    }

    fn get_mock_minf_atom(position: u64) -> MinfAtom {
        MinfAtom {
            _bounds: AtomBounds {
                position,
                size: AtomSize(Size::Standard(112)),
            },
            stbl: get_mock_stbl_atom(position + 8),
        }
    }

    fn get_mock_hdlr_vide_atom(position: u64) -> HdlrAtom {
        HdlrAtom::Vide(AtomBounds {
            position,
            size: AtomSize(Size::Standard(64)),
        })
    }

    fn get_mock_mdhd_atom(position: u64) -> MdhdAtom {
        MdhdAtom {
            _bounds: AtomBounds {
                position,
                size: AtomSize(Size::Standard(32)),
            },
            timescale: 1000,
        }
    }

    fn get_mock_mdia_atom(position: u64) -> MdiaAtom {
        MdiaAtom {
            _bounds: AtomBounds {
                position,
                size: AtomSize(Size::Standard(216)),
            },
            hdlr: get_mock_hdlr_vide_atom(position + 8),
            minf: get_mock_minf_atom(position + 72),
            mdhd: get_mock_mdhd_atom(position + 184),
        }
    }

    fn get_mock_trak_vide_atom(position: u64) -> TrakVideAtom {
        TrakVideAtom {
            _bounds: AtomBounds {
                position,
                size: AtomSize(Size::Standard(316)),
            },
            mdia: get_mock_mdia_atom(position + 8),
            tkhd: get_mock_tkhd_atom(position + 224),
        }
    }

    fn get_mock_hdlr_other_atom(position: u64) -> HdlrAtom {
        HdlrAtom::Other(AtomBounds {
            position,
            size: AtomSize(Size::Standard(128)),
        })
    }

    fn get_mock_mvhd_atom(position: u64) -> MvhdAtom {
        MvhdAtom::V0 {
            _bounds: AtomBounds {
                position,
                size: AtomSize(Size::Standard(28)),
            },
            timescale: 1000,
            duration: 15000,
        }
    }

    fn get_mock_udta_atom(position: u64) -> AtomBounds {
        AtomBounds {
            position,
            size: AtomSize(Size::Standard(768)),
        }
    }

    fn without_atom(
        mut stream: impl io::Read + io::Seek,
        parent_offset: u64,
        atom_offset: u64,
    ) -> (impl io::Read + io::Seek, u32) {
        assert!(atom_offset > parent_offset);

        let mut result = vec![];

        stream
            .by_ref()
            .take(parent_offset)
            .read_to_end(&mut result)
            .expect("Failed to read the test stream");

        let mut size = [0u8; 4];
        stream
            .read_exact(&mut size)
            .expect("Failed to read the parent atom size");
        let old_size = u32::from_be_bytes(size);

        result.extend_from_slice(&size);

        stream
            .by_ref()
            .take(atom_offset - parent_offset - 4)
            .read_to_end(&mut result)
            .expect("Failed to read the test stream");

        stream
            .read_exact(&mut size)
            .expect("Failed to read the child atom size");
        let child_size = u32::from_be_bytes(size);

        stream
            .seek(io::SeekFrom::Current(child_size as i64 - 4))
            .expect("Failed to skip the child atom");

        stream
            .read_to_end(&mut result)
            .expect("Failed to read the test stream");

        let mut result = io::Cursor::new(result);
        result
            .seek(io::SeekFrom::Start(parent_offset))
            .expect("Failed to seek the result stream");

        let new_size = old_size - child_size;

        result
            .write_all(&new_size.to_be_bytes())
            .expect("Failed to write the new parent atom size to the result stream");

        (result, new_size)
    }

    fn with_duplicate_atom(
        mut stream: impl io::Read + io::Seek,
        parent_offset: u64,
        atom_offset: u64,
    ) -> (impl io::Read + io::Seek, u32) {
        assert!(atom_offset > parent_offset);

        let mut result = vec![];

        stream
            .by_ref()
            .take(parent_offset)
            .read_to_end(&mut result)
            .expect("Failed to read the test stream");

        let mut size = [0u8; 4];
        stream
            .read_exact(&mut size)
            .expect("Failed to read the parent atom size");
        let old_size = u32::from_be_bytes(size);
        result.extend_from_slice(&size);

        stream
            .by_ref()
            .take(atom_offset - parent_offset - 4)
            .read_to_end(&mut result)
            .expect("Failed to read the test stream");

        stream
            .read_exact(&mut size)
            .expect("Failed to read the child atom size");
        let child_size = u32::from_be_bytes(size);
        result.extend_from_slice(&size);

        stream
            .by_ref()
            .take(child_size as u64 - 4)
            .read_to_end(&mut result)
            .expect("Failed to read the child atom from the test stream");

        stream
            .seek(io::SeekFrom::Start(atom_offset))
            .expect("Failed to seek the test stream");

        stream
            .read_to_end(&mut result)
            .expect("Failed to read the test stream");

        let new_size = old_size
            .checked_add(child_size)
            .expect("New atom size overflows u64");

        let mut result = io::Cursor::new(result);
        result
            .seek(io::SeekFrom::Start(parent_offset))
            .expect("Failed to seek the result stream");
        result
            .write_all(&new_size.to_be_bytes())
            .expect("Failed to write updated parent atom size into the result stream");

        (result, new_size)
    }

    mod atom_size {
        use super::{AtomSize, Size};
        use crate::SizeError;

        #[test]
        fn standard_atom_size_try_from_zero() {
            let res = AtomSize::try_from(0u32);

            assert!(
                matches!(res, Err(SizeError::EndOfStream)),
                "AtomSize::try_from(0u32) returned an invalid value.\n32-bit value of zero bytes \
                    indicates that the atom continues until the end of the stream and its actual \
                    size should be calculated separately.\n\nExpected: Err(EndOfStream)\nGot:      \
                    {res:?}",
            );
        }

        #[test]
        fn standard_atom_size_try_from_too_small() {
            let res = AtomSize::try_from(4u32);

            assert!(
                matches!(res, Err(SizeError::TooSmall(4))),
                "Call AtomSize::try_from(4u32) returned an invalid value.\n32-bit value of 4 bytes \
                    is invalid, as the minimum possible size for an atom is 8 bytes, representing \
                    an empty atom consisting of only a header.\n\nExpected: Err(TooSmall(4))\n\
                    Got:      {res:?}",
            );
        }

        #[test]
        fn standard_atom_size_try_from_extended() {
            let res = AtomSize::try_from(1u32);

            assert!(
                matches!(res, Err(SizeError::Extended)),
                "Call to AtomSize::try_from(1u32) returned an invalid value.\n32-bit value of 1 \
                    byte indicates that the atom has an extended size field and the actual value \
                    must be read from a different location separtely.\n\nExpected: Err(Extended)\n\
                    Got:      {res:?}",
            );
        }

        #[test]
        fn standard_atom_size_try_from_smallest() {
            let res = AtomSize::try_from(8u32);

            assert!(
                matches!(res, Ok(AtomSize(Size::Standard(8)))),
                "Call to AtomSize:try_from(8u32) returned an invalid value.\n32-bit value of 8 \
                    bytes represents a smallest valid atom size for atoms that consist of only a \
                    header.\n\nExpected: Ok(AtomSize(Standard(8)))\nGot:      {res:?}",
            );
        }

        #[test]
        fn standard_atom_size_try_from() {
            let res = AtomSize::try_from(1024u32);

            assert!(
                matches!(res, Ok(AtomSize(Size::Standard(1024)))),
                "Call to AtomSize::try_from(1024u8) returned an invalid value.\n32-bit value of \
                    1024 bytes is considered a valid standard atom size.\n\nExpected: Ok(AtomSize(
                    Standard(1024)))\nGot:      {res:?}",
            );
        }

        #[test]
        fn extended_atom_size_try_from_zero() {
            let res = AtomSize::try_from(0u64);

            assert!(
                matches!(res, Err(0)),
                "Call to AtomSize::try_from(0u64) returned an invalid value.\n64-bit value of zero \
                    bytes is not valid for an extended atom size.\n\nExpected: Err(0)\nGot:      \
                    {res:?}",
            );
        }

        #[test]
        fn extended_atom_size_try_from_too_small() {
            let res = AtomSize::try_from(8u64);

            assert!(
                matches!(res, Err(8)),
                "Call to AtomSize::try_from(8u64) returned an invalid value.\n64-bit value of 8 \
                    bytes is less than the 16 byte minimum required for an extended atom.\n\n\
                    Expected: Err(8)\nGot:      {res:?}",
            );
        }

        #[test]
        fn extended_atom_size_try_from_smallest() {
            let res = AtomSize::try_from(16u64);

            assert!(
                matches!(res, Ok(AtomSize(Size::Extended(16)))),
                "Call to AtomSize::try_from(16u64) returned an invalid value.\n64-bit value of 16 \
                    bytes represents the minimum size for an extended atom, consisting of a header \
                    and an extended size field.\n\nExpected: Ok(AtomSize(Extended(16)))\n\
                    Got:      {res:?}",
            );
        }

        #[test]
        fn extended_atom_size_try_from() {
            let res = AtomSize::try_from(4294967305u64);

            assert!(
                matches!(res, Ok(AtomSize(Size::Extended(4294967305)))),
                "Call to AtomSize::try_from(4294967305u64) returned an invalid value.\n64-bit \
                    value of 4294967305 is considered valid for an extended atom size.\n\nExpected\
                    : Ok(AtomSize(Extended(4294967305)))\nGot:      {res:?}",
            );
        }

        #[test]
        fn standard_atom_size_display() {
            let size = AtomSize(Size::Standard(2048));

            assert_eq!(
                "2048",
                size.to_string(),
                "The standard atom size did not convert to a string correctly",
            );
        }

        #[test]
        fn extended_atom_size_display() {
            let size = AtomSize(Size::Extended(8589934610));

            assert_eq!(
                "8589934610",
                size.to_string(),
                "The extended atom size did not convert to a string correctly",
            );
        }

        #[test]
        fn standard_atom_size_header_offset() {
            let size = AtomSize(Size::Standard(32));

            assert_eq!(
                8,
                size.header_offset(),
                "An atom with a standard size should always have a content offset of 8 bytes",
            );
        }

        #[test]
        fn extended_atom_size_header_offset() {
            let size = AtomSize(Size::Extended(10737418262));

            assert_eq!(
                16,
                size.header_offset(),
                "An atom with an extended size should always have a content offset of 16 bytes",
            );
        }

        #[test]
        fn standard_atom_size_into_u64() {
            let size = AtomSize(Size::Extended(4096));

            assert_eq!(
                4096,
                u64::from(size),
                "A standard atom size did not properly convert to u64",
            );
        }

        #[test]
        fn extended_atom_size_into_u64() {
            let size = AtomSize(Size::Extended(8053063696));

            assert_eq!(
                8053063696,
                u64::from(size),
                "An extended atom size did not properly convert to u64",
            );
        }

        #[test]
        fn end_of_stream_atom_size_header_offset() {
            let size = AtomSize(Size::EndOfStream(65532));

            assert_eq!(
                8,
                size.header_offset(),
                "An atom with an end-of-stream size should always have a content offset of 8 bytes",
            );
        }

        #[test]
        fn end_of_stream_atom_size_into_u64() {
            let size = AtomSize(Size::EndOfStream(11453246120));

            assert_eq!(
                11453246120,
                u64::from(size),
                "An end-of-stream atom size did not properly convert to u64",
            );
        }

        #[test]
        fn end_of_stream_atom_size_display() {
            let size = AtomSize(Size::EndOfStream(7516192766));

            assert_eq!(
                "7516192766",
                size.to_string(),
                "An end-of-stream atom size did not convert to a string correctly",
            );
        }
    }

    mod atom_type {
        use super::AtomType;

        #[test]
        fn moov_atom_parsing() {
            let atom = AtomType::from(b"moov".to_owned());

            assert_eq!(
                AtomType::Moov,
                atom,
                "A moov atom type did not parse into correct enum variant",
            );
        }

        #[test]
        fn moov_atom_to_string() {
            assert_eq!(
                "moov",
                AtomType::Moov.to_string(),
                "A moov atom type did not serialise into a string correctly",
            );
        }

        #[test]
        fn mvhd_atom_parsing() {
            let atom = AtomType::from(b"mvhd".to_owned());

            assert_eq!(
                AtomType::Mvhd,
                atom,
                "An mvhd atom type did not parse into correct enum variant",
            );
        }

        #[test]
        fn mvhd_atom_to_string() {
            assert_eq!(
                "mvhd",
                AtomType::Mvhd.to_string(),
                "An mvhd atom type did not serialise into a string correctly",
            );
        }

        #[test]
        fn trak_atom_parsing() {
            let atom = AtomType::from(b"trak".to_owned());

            assert_eq!(
                AtomType::Trak,
                atom,
                "A trak atom type did not parse into correct enum variant",
            );
        }

        #[test]
        fn trak_atom_to_string() {
            assert_eq!(
                "trak",
                AtomType::Trak.to_string(),
                "A trak atom type did not serialise into a string correctly",
            );
        }

        #[test]
        fn udta_atom_parsing() {
            let atom = AtomType::from(b"udta".to_owned());

            assert_eq!(
                AtomType::Udta,
                atom,
                "A udta atom type did not parse into correct enum variant",
            );
        }

        #[test]
        fn udta_atom_to_string() {
            assert_eq!(
                "udta",
                AtomType::Udta.to_string(),
                "A udta atom type did not serialise into a string correctly",
            );
        }

        #[test]
        fn mdia_atom_parsing() {
            let atom = AtomType::from(b"mdia".to_owned());

            assert_eq!(
                AtomType::Mdia,
                atom,
                "An mdia atom type did not parse into correct enum variant",
            );
        }

        #[test]
        fn mdia_atom_to_string() {
            assert_eq!(
                "mdia",
                AtomType::Mdia.to_string(),
                "An mdia atom type did not serialise into a string correctly",
            );
        }

        #[test]
        fn tkdh_atom_parsing() {
            let atom = AtomType::from(b"tkhd".to_owned());

            assert_eq!(
                AtomType::Tkhd,
                atom,
                "A tkhd atom type did not parse into correct enum variant",
            );
        }

        #[test]
        fn tkhd_atom_to_string() {
            assert_eq!(
                "tkhd",
                AtomType::Tkhd.to_string(),
                "A tkhd atom type did not serialise into a string correctly",
            );
        }

        #[test]
        fn hdlr_atom_parsing() {
            let atom = AtomType::from(b"hdlr".to_owned());

            assert_eq!(
                AtomType::Hdlr,
                atom,
                "A hdlr atom type did not parse into correct enum variant",
            );
        }

        #[test]
        fn hdlr_atom_to_string() {
            assert_eq!(
                "hdlr",
                AtomType::Hdlr.to_string(),
                "An hdlr atom type did not serialise into a string correctly",
            );
        }

        #[test]
        fn minf_atom_parsing() {
            let atom = AtomType::from(b"minf".to_owned());

            assert_eq!(
                AtomType::Minf,
                atom,
                "A minf atom type did not parse into correct enum variant",
            );
        }

        #[test]
        fn minf_atom_to_string() {
            assert_eq!(
                "minf",
                AtomType::Minf.to_string(),
                "A minf atom type did not serialise into a string correctly",
            );
        }

        #[test]
        fn stbl_atom_parsing() {
            let atom = AtomType::from(b"stbl".to_owned());

            assert_eq!(
                AtomType::Stbl,
                atom,
                "An stbl atom type did not parse into correct enum variant",
            );
        }

        #[test]
        fn stbl_aotm_to_string() {
            assert_eq!(
                "stbl",
                AtomType::Stbl.to_string(),
                "An stbl atom type did not serialise into a string correctly",
            );
        }

        #[test]
        fn stts_atom_pasing() {
            let atom = AtomType::from(b"stts".to_owned());

            assert_eq!(
                AtomType::Stts,
                atom,
                "An stts atom type did not parse into correct enum variant",
            );
        }

        #[test]
        fn stts_atom_to_string() {
            assert_eq!(
                "stts",
                AtomType::Stts.to_string(),
                "An stts atom type did not serialise into a string correctly",
            );
        }

        #[test]
        fn mdhd_atom_parsing() {
            let atom = AtomType::from(b"mdhd".to_owned());

            assert_eq!(
                AtomType::Mdhd,
                atom,
                "An mdhd atom type did not parse into correct enum variant",
            );
        }

        #[test]
        fn mdhd_atom_to_string() {
            assert_eq!(
                "mdhd",
                AtomType::Mdhd.to_string(),
                "An mdhd atom type did not serialise into a string correctly",
            );
        }

        #[test]
        fn ftyp_atom_parsing() {
            let atom = AtomType::from(b"ftyp".to_owned());

            assert_eq!(
                AtomType::Other([102, 116, 121, 112]),
                atom,
                "An ftyp atom type did not parse into correct enum variant",
            );
        }

        #[test]
        fn ftyp_atom_to_string() {
            assert_eq!(
                "ftyp",
                AtomType::Other(b"ftyp".to_owned()).to_string(),
                "An ftyp atom type did not serialise into a string correctly",
            );
        }

        #[test]
        fn binary_atom_parsing() {
            let atom = AtomType::from([0, 75, 150, 225].to_owned());

            assert_eq!(
                AtomType::Other([0, 75, 150, 225]),
                atom,
                "A binary atom type did not parse into correct enum variant",
            );
        }

        #[test]
        fn binary_atom_to_string() {
            assert_eq!(
                "[00 4b 96 e1]",
                AtomType::Other([0, 75, 150, 225]).to_string(),
                "A binary atom type did not serialise into a string correctly",
            );
        }
    }

    mod atom_bounds {
        use super::{AtomBounds, AtomSize, Size};

        #[test]
        fn ends_at_standard_atom() {
            let bounds = AtomBounds {
                position: 32,
                size: AtomSize(Size::Standard(2048)),
            };

            assert_eq!(
                Some(2080),
                bounds.ends_at(),
                "AtomBounds::ends_at() returned an invalid value for a stadard size atom",
            );
        }

        #[test]
        fn ends_at_extended_atom() {
            let bounds = AtomBounds {
                position: 32,
                size: AtomSize(Size::Extended(10737418269)),
            };

            assert_eq!(
                Some(10737418301),
                bounds.ends_at(),
                "AtomBounds::ends_at() returned an invalid value for an extended size atom",
            );
        }

        #[test]
        fn ends_at_standard_overflow() {
            let bounds = AtomBounds {
                position: u64::MAX,
                size: AtomSize(Size::Standard(32703)),
            };

            assert_eq!(
                None,
                bounds.ends_at(),
                "AtomBounds::ends_at() returned an invalid value for a standard size atom that \
                    starts at u64::MAX position and extends beyond the 64-bit limit",
            );
        }

        #[test]
        fn ends_at_extended_overflow() {
            let bounds = AtomBounds {
                position: u64::MAX,
                size: AtomSize(Size::Extended(11166914967)),
            };

            assert_eq!(
                None,
                bounds.ends_at(),
                "AtomBounds::ends_at() returned an invalid value for an extended size atom that \
                    starts at u64::MAX position and extends beyond the 64-bit limit",
            );
        }

        #[test]
        fn ends_at_end_of_stream_overflow() {
            let bounds = AtomBounds {
                position: u64::MAX,
                size: AtomSize(Size::EndOfStream(128436)),
            };

            assert_eq!(
                None,
                bounds.ends_at(),
                "AtomBounds::ends_at() returned an invalid value for atom that starts at u64::MAX \
                    position and continues to the end of the stream",
            );
        }

        #[test]
        fn content_position_standard_atom() {
            let bounds = AtomBounds {
                position: 39843,
                size: AtomSize(Size::Standard(32703)),
            };

            assert_eq!(
                Some(39851),
                bounds.content_position(),
                "AtomBounds::content_position() returned an invalid value for a standard size atom",
            );
        }

        #[test]
        fn content_position_extended_atom() {
            let bounds = AtomBounds {
                position: 123908,
                size: AtomSize(Size::Extended(9817068102)),
            };

            assert_eq!(
                Some(123924),
                bounds.content_position(),
                "AtomBounds::content_position() returned an invalid value for an extended size \
                    atom",
            );
        }

        #[test]
        fn content_position_end_of_stream_atom() {
            let bounds = AtomBounds {
                position: 853432,
                size: AtomSize(Size::EndOfStream(855421)),
            };

            assert_eq!(
                Some(853440),
                bounds.content_position(),
                "AtomBounds::content_position() returned an invalid value for an atom that spans \
                    until the end of stream",
            );
        }

        #[test]
        fn content_position_standard_atom_overflow() {
            let bounds = AtomBounds {
                position: 18446744073709551612,
                size: AtomSize(Size::Standard(1043)),
            };

            assert_eq!(
                None,
                bounds.content_position(),
                "AtomBounds::content_position() returned an invalid value for a standard size atom \
                    starting near the u64::MAX position with content located beyond the 64-bit \
                    limit",
            );
        }

        #[test]
        fn content_position_extended_atom_overflow() {
            let bounds = AtomBounds {
                position: 18446744073709551610,
                size: AtomSize(Size::Extended(4073709551613)),
            };

            assert_eq!(
                None,
                bounds.content_position(),
                "AtomBounds::content_position() returned an invalid value for an extended size \
                    atom starting near the u64::MAX position with content located beyond the \
                    64-bit limit",
            );
        }

        #[test]
        fn content_position_end_of_stream_atom_overflow() {
            let bounds = AtomBounds {
                position: 18446744073709551614,
                size: AtomSize(Size::Extended(3548934)),
            };

            assert_eq!(
                None,
                bounds.content_position(),
                "AtomBounds::content_position() returned an invalid value for an extended size \
                    atom that starts near the u64::MAX position with contents position extending \
                    beyond the 64-bit limit",
            );
        }
    }

    mod atom_header {
        use super::{
            super::{AtomBounds, AtomHeader, AtomSize, AtomType, ParseError, Size},
            MockStream, TestStream,
        };
        // use super::{
        //     super::ParseError, AtomBounds, AtomHeader, AtomSize, AtomType, Size, TestStream,
        //     UdtaAtom,
        // };
        use mockall::predicate;
        use std::io::{self, Seek, Write as _};

        #[test]
        fn parse_from_stream_current_position_error() {
            let mut stream = MockStream::new();

            stream
                .expect_seek()
                .with(predicate::eq(io::SeekFrom::Current(0)))
                .return_once(|_| Err(io::Error::other("Expected seek failure")));

            let res = AtomHeader::parse_from_stream(&mut stream);

            assert!(
                matches!(
                    res,
                    Err(ParseError::CurrentStreamPosition(ref e)) if e.kind() == io::ErrorKind::Other
                        && e.to_string() == "Expected seek failure",
                ),
                "AtomHeader::parse_from_stream() returned an invalid value.\nExpected the mocked \
                    test error to be returned when getting the current stream position.\n\nExpected\
                    : Err(CurrentStreamPosition(Custom {{ kind: Other, error: \"Expected seek \
                    failure\" }}))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_read_error() {
            let mut stream = io::Cursor::new([0; 2]);

            let res = AtomHeader::parse_from_stream(&mut stream);

            assert!(
                matches!(
                    res,
                    Err(ParseError::Read(ref e)) if e.kind() == io::ErrorKind::UnexpectedEof,
                ),
                "AtomHeader::parse_from_stream() returned an invalid value.\nTrying to read an 8 \
                    byte header from a shorter stream should fail with the unexpected end of file \
                    error.\n\nExpected: Err(Read(Error({{ kind: UnexpectedEof, message: \"failed \
                    to fill whole buffer\" }})))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_standard_atom_size_too_small() {
            let mut stream = io::Cursor::new([
                0, 0, 0, 4, // size
                109, 111, 111, 118, // type: moov
            ]);

            let res = AtomHeader::parse_from_stream(&mut stream);

            assert!(
                matches!(res, Err(ParseError::AtomSize(8, 4))),
                "AtomHeader::parse_from_stream() returned an invalid value.\nAn atom size field \
                    indicates the size of 4 bytes, but the minimum valid size for any atom is 8 \
                    bytes.\n\nExpected: Err(AtomSize(8, 4))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_end_of_stream_position_error() {
            let mut stream = MockStream::new();

            let mut seq = mockall::Sequence::new();

            stream
                .expect_seek()
                .with(predicate::eq(io::SeekFrom::Current(0)))
                .once()
                .in_sequence(&mut seq)
                .return_once(|_| Ok(0));

            stream
                .expect_seek()
                .with(predicate::eq(io::SeekFrom::Current(0)))
                .once()
                .in_sequence(&mut seq)
                .return_once(|_| Err(io::Error::other("Expected test error")));

            stream.expect_read().return_once(|buf| {
                let mut cur = io::Cursor::new(buf);

                cur.write(&[
                    0, 0, 0, 0, // size
                    109, 111, 111, 118, // type: moov
                ])
            });

            let res = AtomHeader::parse_from_stream(&mut stream);

            assert!(
                matches!(
                    res,
                    Err(ParseError::CurrentStreamPosition(ref e))
                        if e.kind() == io::ErrorKind::Other
                        && e.to_string() == "Expected test error",
                ),
                "AtomHeader::parse_from_stream() returned an invalid value.\nExpected a mocked \
                    error to be returned when calculating the end of stream.\n\nExpected: Err(\
                    CurrentStreamPosition(Custom {{ kind: Other, error: \"Expected test error\" \
                    }}))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_extended_atom_size_read_error() {
            let mut stream = io::Cursor::new([
                0, 0, 0, 1, // size
                109, 111, 111, 118, // type: moov
                0, 0, 0, 5, // extended size (truncated)
            ]);

            let res = AtomHeader::parse_from_stream(&mut stream);

            assert!(
                matches!(
                    res,
                    Err(ParseError::Read(ref e)) if e.kind() == io::ErrorKind::UnexpectedEof,
                ),
                "AtomHeader::parse_from_stream() returned an invalid value.\nA size field value of \
                    1 indicates an extended atom, but the stream ends before the required 8-byte \
                    extended size field can be read.\n\nExpected: Err(Read(Error {{ kind: \
                    UnexpectedEof, message: \"failed to fill whole buffer\" }}))\nGot:      \
                    {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_extended_atom_size_too_small() {
            let mut stream = io::Cursor::new([
                0, 0, 0, 1, // size
                109, 111, 111, 118, // type: moov
                0, 0, 0, 0, 0, 0, 0, 10, // extended size
            ]);

            let res = AtomHeader::parse_from_stream(&mut stream);

            assert!(
                matches!(res, Err(ParseError::AtomSize(16, 10))),
                "AtomHeader::parse_from_stream() returned an invalid value.\nAn extended atom size \
                    field of 10 bytes is invalid, as the minimum required size for an extended \
                    atom is 16 bytes.\n\nExpected: Err(AtomSize(16, 10))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_standard_size_atom() {
            let mut stream = io::Cursor::new([
                0, 0, 0, 0, // garbage
                0, 0, 0, 12, // size
                109, 111, 111, 118, // type: moov
                0, 0, 0, 0,
            ]);

            stream
                .seek(io::SeekFrom::Start(4))
                .expect("Failed to seek the test stream");

            let res = AtomHeader::parse_from_stream(&mut stream);

            let header = res.expect(
                "Expected AtomHeader::parse_from_stream() to return the Ok() variant when the \
                    stream contains a valid atom header",
            );

            assert_eq!(
                AtomType::Moov,
                header.atom_type,
                "AtomHeader::parse_from_stream() did not return a valid atom type",
            );

            assert_eq!(
                4,
                header.position(),
                "AtomHeader::parse_from_stream() did not return a valid starting position for the \
                    atom",
            );

            assert_eq!(
                12,
                u64::from(header.size()),
                "AtomHeader::parse_from_stream() did not return a valid size for a standard size \
                    atom",
            );
        }

        #[test]
        fn parse_from_stream_extended_size_atom() {
            let mut stream = io::Cursor::new([
                0, 0, 0, 1, // size
                109, 111, 111, 118, // type: moov
                0, 0, 0, 1, 0, 0, 0, 0, // extended size
            ]);

            let res = AtomHeader::parse_from_stream(&mut stream);

            let header = res.expect(
                "Expected AtomHeader::parse_from_stream() to return the Ok() variant when the \
                    stream contains a valid atom header and the extended size field",
            );

            assert_eq!(
                AtomType::Moov,
                header.atom_type,
                "AtomHeader::parse_from_stream() did not return a valid atom type",
            );

            assert_eq!(
                0,
                header.position(),
                "AtomHeader::parse_from_stream() did not return a valid starting position for the \
                    atom",
            );

            assert_eq!(
                4294967296,
                u64::from(header.size()),
                "AtomHeader::parse_from_stream() did not return a valid size for an extended size \
                    atom",
            );
        }

        #[test]
        fn parse_from_stream_end_of_stream_size_atom() {
            let mut stream = io::Cursor::new([
                0, 0, 0, 0, // size
                109, 111, 111, 118, // type: moov
                0, 0, 0, 8, 13, 6, 4, 15, 7, 9, 3, 11, 44, 8, 13,
            ]);

            let res = AtomHeader::parse_from_stream(&mut stream);

            let header = res.expect(
                "Expected AtomHeader::parse_from_stream() to return the Ok() variant when the \
                    stream contains a valid atom header with the zero value in its size field",
            );

            assert_eq!(
                AtomType::Moov,
                header.atom_type,
                "AtomHeader::parse_from_stream() did not return a valid atom type",
            );

            assert_eq!(
                0,
                header.position(),
                "AtomHeader::parse_from_stream() did not return a valid atom starting position",
            );

            assert_eq!(
                23,
                u64::from(header.size()),
                "AtomHeader::parse_from_stream() did not return a valid size for the atom that \
                    extends to the end of the stream",
            );
        }

        #[test]
        fn ends_at_for_standard_size_atom() {
            let header = AtomHeader {
                bounds: AtomBounds {
                    position: 32,
                    size: AtomSize(Size::Standard(64)),
                },
                atom_type: AtomType::Mdia,
            };

            let res = header.ends_at().expect(
                "Expected AtomHeader::ends_at() to return the Ok() variant for a valid atom \
                    header",
            );

            assert_eq!(
                96, res,
                "AtomHeader::ends_at() did not return a valid value for a standard size atom",
            );
        }

        #[test]
        fn ends_at_for_standard_size_atom_overflow() {
            let header = AtomHeader {
                bounds: AtomBounds {
                    position: u64::MAX,
                    size: AtomSize(Size::Standard(u32::MAX)),
                },
                atom_type: AtomType::Trak,
            };

            let res = header.ends_at();

            assert!(
                matches!(res, Err(ParseError::MathError(AtomType::Trak))),
                "AtomHeader::ends_at() returned an invalid value.\nAn atom starting at u64::MAX is \
                    invalid as its content extends beyond the 64-bit limit.\n\nExpected: \
                    Err(MathError(Trak))\nGot:      {res:?}",
            );
        }

        #[test]
        fn content_position_for_standard_atom_size() {
            let header = AtomHeader {
                bounds: AtomBounds {
                    position: 9833,
                    size: AtomSize(Size::EndOfStream(10482)),
                },
                atom_type: AtomType::Stbl,
            };

            let res = header.content_position().expect(
                "Expected AtomHeader::content_position() to return the Ok() variant for a \
                    valid atom header that spans until the end of the stream",
            );

            assert_eq!(
                9841, res,
                "AtomHeader::content_position() returned an invalid value for a valid atom header \
                    that spans until the end of the stream.",
            );
        }

        #[test]
        fn content_position_for_overflowing_atom() {
            let header = AtomHeader {
                bounds: AtomBounds {
                    position: u64::MAX,
                    size: AtomSize(Size::Standard(8342)),
                },
                atom_type: AtomType::Moov,
            };

            let res = header.content_position();

            assert!(
                matches!(res, Err(ParseError::MathError(AtomType::Moov))),
                "AtomHeader::content_position() returned an invalid value.\nThe content position \
                    for an atom starting at u64::MAX is invalid as it extends beyond the 64-bit \
                    limit.\n\nExpected: Err(MathError(Moov))\nGot:      {res:?}",
            );
        }

        #[test]
        fn skip_overflowing_atom() {
            let header = AtomHeader {
                bounds: AtomBounds {
                    position: 32,
                    size: AtomSize(Size::Extended(u64::MAX)),
                },
                atom_type: AtomType::Other(b"ftyp".to_owned()),
            };

            let mut stream = io::Cursor::new([0u8; 0]);

            let res = header.skip(&mut stream);

            assert!(
                matches!(
                    res,
                    Err(ParseError::MathError(AtomType::Other([102, 116, 121, 112]))),
                ),
                "AtomHeader::skip() returned an invalid value.\nAn extended atom starting at 32 \
                    bytes with a size of u64::MAX extends beyond the 64-bit limit.\n\nExpected: \
                    Err(MathError(Other([102, 116, 121, 112])))\nGot:      {res:?}",
            );
        }

        #[test]
        fn skip_seek_error() {
            let header = AtomHeader {
                bounds: AtomBounds {
                    position: 0,
                    size: AtomSize(Size::Standard(8)),
                },
                atom_type: AtomType::Other(b"ftyp".to_owned()),
            };

            let mut stream = MockStream::new();

            stream
                .expect_seek()
                .with(predicate::eq(io::SeekFrom::Start(8)))
                .return_once(|_| Err(io::Error::other("Expected seek failure")));

            let res = header.skip(&mut stream);

            assert!(
                matches!(
                    res,
                    Err(ParseError::Seek(ref err))
                        if err.kind() == io::ErrorKind::Other
                            && err.to_string() == "Expected seek failure",
                ),
                "AtomHeader::skip() returned an invalid value.\nIt should return a mocked error \
                    when unable to seek the stream.\n\nExpected: Err(Seek(Custom {{ kind: Other, \
                    error: \"Expected seek failure\" }}))\nGot:      {res:?}",
            );
        }

        #[test]
        fn skip() {
            let header = AtomHeader {
                bounds: AtomBounds {
                    position: 0,
                    size: AtomSize(Size::Standard(16)),
                },
                atom_type: AtomType::Moov,
            };

            let mut stream = io::Cursor::new([
                0, 0, 0, 16, // size
                109, 111, 111, 118, // type: moov
                0, 12, 13, 52, 53, 98, 115, 236,
            ]);

            let res = header.skip(&mut stream).expect(
                "Expected AtomHeader::skip() to return the Ok() variant when it successfully skips \
                    an atom",
            );

            assert_eq!(
                res, 16,
                "AtomHeader::skip() did not return a valid new stream position after the atom was \
                    skipped",
            );
        }

        #[test]
        fn parse_container_content_overflow_error() {
            let header = AtomHeader {
                bounds: AtomBounds {
                    position: u64::MAX - 4,
                    size: AtomSize(Size::Standard(8423)),
                },
                atom_type: AtomType::Trak,
            };

            let mut stream = io::Cursor::new([0u8; 0]);

            let res = header.parse_container(Some(()), &mut stream, |_, _, _| unimplemented!());

            assert!(
                matches!(res, Err(ParseError::MathError(AtomType::Trak))),
                "AtomHeader::parse_container() returned an invalid value.\nThe starting position \
                    of the atom content cannot be determined because it starts beyond the 64-bit \
                    limit, even though the atom header begins within bounds.\n\nExpected: \
                    Err(MathError(Trak))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_container_atom_overflow_error() {
            let header = AtomHeader {
                bounds: AtomBounds {
                    position: u64::MAX - 100,
                    size: AtomSize(Size::Standard(101)),
                },
                atom_type: AtomType::Minf,
            };

            let mut stream = io::Cursor::new([0u8; 0]);

            let res = header.parse_container(Some(()), &mut stream, |_, _, _| unimplemented!());

            assert!(
                matches!(res, Err(ParseError::MathError(AtomType::Minf))),
                "AtomHeader::parse_container() returned an invalid value.\nThe atom cannot be \
                    parsed because its end position extends beyond the 64-bit limit.\n\nExpected: \
                    Err(MathError(Minf))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_container_seek_error() {
            let mut stream = MockStream::new();

            stream
                .expect_seek()
                .with(predicate::eq(io::SeekFrom::Start(12353)))
                .return_once(|_| Err(io::Error::other("Expected seek failure")));

            let header = AtomHeader {
                bounds: AtomBounds {
                    position: 12345,
                    size: AtomSize(Size::Standard(512)),
                },
                atom_type: AtomType::Hdlr,
            };

            let res = header.parse_container(Some(()), &mut stream, |_, _, _| unimplemented!());

            assert!(
                matches!(&res, Err(ParseError::Seek(err)) if err.kind() == io::ErrorKind::Other &&
                    err.to_string() == "Expected seek failure"),
                "AtomHeader::parse_container() returned an invalid value.\nThe mocked error should \
                    be returned when the stream seeking fails to advance to the position of the \
                    atom content.\n\nExpected: Err(Seek(Custom {{ kind: Other, error: \"Expected \
                    seek failure\" }})\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_container_stream_position_error() {
            let mut stream = MockStream::new();

            stream
                .expect_seek()
                .with(predicate::eq(io::SeekFrom::Start(8778)))
                .return_once(|_| Ok(8778));

            stream
                .expect_seek()
                .with(predicate::eq(io::SeekFrom::Current(0)))
                .return_once(|_| Err(io::Error::other("mock error")));

            let header = AtomHeader {
                bounds: AtomBounds {
                    position: 8770,
                    size: AtomSize(Size::Standard(1024)),
                },
                atom_type: AtomType::Udta,
            };

            let res = header.parse_container(Some(()), &mut stream, |_, _, _| unimplemented!());

            assert!(
                matches!(
                    &res,
                    Err(ParseError::CurrentStreamPosition(err))
                        if err.kind() == io::ErrorKind::Other && err.to_string() == "mock error",
                ),
                "AtomHeader::parse_container() returned an invalid value.\nThe mocked error should \
                    be returned when the current stream posision cannot be obtained.\n\nErr(\
                    CurrentStreamPosition(Custom {{ kind: Other, error: \"mock error\" }}))\n\
                    Got:      {res:?}",
            );
        }

        #[test]
        fn parse_container_child_atom_parse_error() {
            let mut stream = MockStream::new();

            stream
                .expect_seek()
                .with(predicate::eq(io::SeekFrom::Start(37397)))
                .return_once(|_| Ok(36397));

            stream
                .expect_seek()
                .with(predicate::eq(io::SeekFrom::Current(0)))
                .times(1)
                .return_once(|_| Ok(36397));

            stream
                .expect_seek()
                .with(predicate::eq(io::SeekFrom::Current(0)))
                .times(1)
                .return_once(|_| Err(io::Error::other("test error")));

            let header = AtomHeader {
                bounds: AtomBounds {
                    position: 37389,
                    size: AtomSize(Size::Standard(831312)),
                },
                atom_type: AtomType::Moov,
            };

            let res = header.parse_container(Some(()), &mut stream, |_, _, _| unimplemented!());

            assert!(
                matches!(
                    res,
                    Err(ParseError::CurrentStreamPosition(ref err))
                    if err.kind() == io::ErrorKind::Other && err.to_string() == "test error",
                ),
                "AtomHeader::parse_container() returned an invalid value.\nThe mocked error should \
                    be returned when the child atom header cannot be parsed.\n\nExpected: \
                    Err(CurrentStreamPosition(Custom {{ kind: Other, error: \"test error\" }}))\n\
                    Got:      {res:?}",
            );
        }

        #[test]
        fn parse_container_child_atom_overflow_error() {
            let mut stream = io::Cursor::new([
                0, 0, 0, 8, 84, 69, 83, 84, // garbage: 8 byte TEST atom
                0, 0, 0, 24, // size
                109, 105, 110, 102, // type: minf
                0, 0, 0, 1, 118, 109, 104, 100, 255, 255, 255, 255, 255, 255, 255, 250,
            ]);

            let header = AtomHeader {
                bounds: AtomBounds {
                    position: 8,
                    size: AtomSize(Size::Standard(24)),
                },
                atom_type: AtomType::Minf,
            };

            let res = header.parse_container(Some(()), &mut stream, |_, _, _| unimplemented!());

            assert!(
                matches!(
                    res,
                    Err(ParseError::MathError(AtomType::Other([118, 109, 104, 100]))),
                ),
                "AtomHeader::parse_container() returned an invalid value.\nParsing should fail \
                    because the child atom end position extends beyond the 64-bit limit.\n\n\
                    Expected: Err(MathError(Other([118, 109, 104, 100])))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_container_child_atom_overlap_error() {
            let mut stream = io::Cursor::new([
                0, 0, 0, 8, 84, 69, 83, 84, // garbage: 8 byte TEST atom
                0, 0, 0, 24, // size
                109, 105, 110, 102, // type: minf
                0, 0, 0, 32, 109, 118, 104, 100, 213, 38, 123, 33, 38,
            ]);

            let header = AtomHeader {
                bounds: AtomBounds {
                    position: 8,
                    size: AtomSize(Size::Standard(24)),
                },
                atom_type: AtomType::Minf,
            };

            let res = header.parse_container(Some(()), &mut stream, |_, _, _| unimplemented!());

            assert!(
                matches!(res, Err(ParseError::SizeOverlap(AtomType::Mvhd))),
                "AtomHeader::parse_container() returned an invalid value.\nParsing should fail \
                    because the child atom extends beyond the parent atom's bounds.\n\nExpected: \
                    Err(SizeOverlap(Mvhd))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_container_child_skip_error() {
            struct MockStream(io::Cursor<[u8; 29]>);

            impl io::Seek for MockStream {
                fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
                    if let io::SeekFrom::Start(32) = pos {
                        return Err(io::Error::other("test atom skip error"));
                    }

                    self.0.seek(pos)
                }
            }

            impl io::Read for MockStream {
                fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                    self.0.read(buf)
                }
            }

            let stream = io::Cursor::new([
                0, 0, 0, 8, 84, 69, 83, 84, // garbage: 8 byte TEST atom
                0, 0, 0, 24, // size
                109, 105, 110, 102, // type: minf
                0, 0, 0, 16, 109, 118, 104, 100, 213, 38, 123, 33, 38,
            ]);

            let mut stream = MockStream(stream);

            let header = AtomHeader {
                bounds: AtomBounds {
                    position: 8,
                    size: AtomSize(Size::Standard(24)),
                },
                atom_type: AtomType::Minf,
            };

            let res = header.parse_container(None, &mut stream, |_, _, _| Ok(Some(())));

            assert!(
                matches!(
                    &res,
                    Err(ParseError::Seek(e)) if e.kind() == io::ErrorKind::Other
                        && e.to_string() == "test atom skip error",
                ),
                "AtomHeader::parse_container() returned an invalid value.\nParsing should fail \
                    with the mocked error when seeking past the child atom fails.\n\nExpected: \
                    Err(AtomVersion(213, Mvhd))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_container_child_atom_callback_error() {
            let mut stream = io::Cursor::new([
                0, 0, 0, 8, 84, 69, 83, 84, // garbage: 8 byte TEST atom
                0, 0, 0, 24, // size
                109, 105, 110, 102, // type: minf
                0, 0, 0, 16, 109, 118, 104, 100, 213, 38, 123, 33, 38, 48, 92, 10,
            ]);

            let header = AtomHeader {
                bounds: AtomBounds {
                    position: 8,
                    size: AtomSize(Size::Standard(24)),
                },
                atom_type: AtomType::Minf,
            };

            let res = header.parse_container(Some(()), &mut stream, |_, _, _| {
                Err(ParseError::AtomVersion(213, AtomType::Mvhd))
            });

            assert!(
                matches!(res, Err(ParseError::AtomVersion(213, AtomType::Mvhd))),
                "AtomHeader::parse_container() returned an invalid value.\nParsing should fail and \
                    bubble up the error returned by the child atom parsing callback.\n\nExpected: \
                    Err(AtomVersion(213, Mvhd))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_container() {
            let mut stream = io::Cursor::new([
                0, 0, 0, 8, 84, 69, 83, 84, // garbage: 8 byte TEST atom
                0, 0, 0, 24, // size
                109, 105, 110, 102, // type: minf
                0, 0, 0, 16, 109, 118, 104, 100, 213, 38, 123, 33, 38, 48, 92, 10,
            ]);

            let header = AtomHeader {
                bounds: AtomBounds {
                    position: 8,
                    size: AtomSize(Size::Standard(24)),
                },
                atom_type: AtomType::Minf,
            };

            let res = header.parse_container(None, &mut stream, |_, _, _| Ok(Some(())));

            assert!(
                matches!(res, Ok(Some(()))),
                "AtomHeader::parse_container() returned an invalid value.\nParsing should succeed \
                    when the input stream contains valid data.\n\nExpected: Ok(Some(()))\n\
                    Got:      {res:?}",
            );
        }

        fn get_test_udta_stream<OnRead>(on_read: OnRead) -> impl io::Read + io::Seek
        where
            OnRead: Fn(&mut io::Cursor<Vec<u8>>) -> io::Result<()>,
        {
            let stream = io::Cursor::new(vec![
                0, 0, 0, 32, // size 32 bytes
                117, 100, 116, 97, // type: udta
                0, 0, 0, 8, // size: 8
                115, 97, 118, 111, // type: savo
                0, 0, 0, 8, // size: 8
                100, 117, 112, 101, // type: dupe
                0, 0, 0, 8, // size: 8
                100, 117, 112, 101, // type: dupe
            ]);

            TestStream { stream, on_read }
        }

        #[test]
        fn find_children_fails_if_container_parse_returns_error() {
            let mut stream = get_test_udta_stream(|stream| {
                (stream.position() != 8)
                    .then_some(())
                    .ok_or(io::Error::other("atom parse err"))
            });

            let header = AtomHeader {
                bounds: AtomBounds {
                    position: 0,
                    size: AtomSize(Size::Standard(32)),
                },
                atom_type: AtomType::Udta,
            };

            let res = header.find_children(&mut stream, AtomType::Other([b's', b'a', b'v', b'o']));

            assert!(
                matches!(
                    res,
                    Err(ParseError::Read(ref e)) if e.kind() == io::ErrorKind::Other
                        && e.to_string() == "atom parse err",
                ),
                "AtomHeader::find_children() returned an invalid value.\nThe parser should fail \
                    when the atom content cannot be read.\n\nExpected: Err(Read(Custom {{ kind: \
                    Other, error: \"atom parse err\" }}))\nGot:      {res:?}",
            );
        }

        #[test]
        fn find_children_returns_no_atoms_if_none_found() {
            let mut stream = get_test_udta_stream(|_| Ok(()));

            let header = AtomHeader {
                bounds: AtomBounds {
                    position: 0,
                    size: AtomSize(Size::Standard(32)),
                },
                atom_type: AtomType::Udta,
            };

            let res = header.find_children(&mut stream, AtomType::Other([b'm', b'i', b's', b's']));

            assert!(
                matches!(&res, Ok(r) if r.is_empty()),
                "AtomHeader::find_children() returned an invalid value.\nThe parser should return \
                    an empty vector when no child 'miss' atoms are found.\n\nExpected: Ok([])\n\
                    Got:      {res:?}",
            );
        }

        #[test]
        fn find_children_returns_found_atoms() {
            let mut stream = get_test_udta_stream(|_| Ok(()));

            let header = AtomHeader {
                bounds: AtomBounds {
                    position: 0,
                    size: AtomSize(Size::Standard(32)),
                },
                atom_type: AtomType::Udta,
            };

            let res = header.find_children(&mut stream, AtomType::Other([b'd', b'u', b'p', b'e']));

            assert!(
                matches!(&res, Ok(r) if r.len() == 2),
                "AtomHeader::find_children() returned an invalid value.\nThe parser should return \
                    two 'dupe' child atoms.\n\nExpected: Ok([AtomHeader {{ .. }}, AtomHeader {{ .. \
                    }}])\nGot:      {res:?}",
            );
        }

        #[test]
        fn find_child_returns_none_if_none_found() {
            let mut stream = get_test_udta_stream(|_| Ok(()));

            let header = AtomHeader {
                bounds: AtomBounds {
                    position: 0,
                    size: AtomSize(Size::Standard(32)),
                },
                atom_type: AtomType::Udta,
            };

            let res = header.find_child(&mut stream, AtomType::Other([b'n', b'o', b'n', b'e']));

            assert!(
                matches!(&res, Ok(None)),
                "AtomHeader::find_child() returned an invalid value.\nThe parser should return a \
                    None when no child atoms are found.\n\nExpected: Ok(None)\nGot:      {res:?}",
            );
        }

        #[test]
        fn find_child_returns_error_if_duplicates_found() {
            let mut stream = get_test_udta_stream(|_| Ok(()));

            let header = AtomHeader {
                bounds: AtomBounds {
                    position: 0,
                    size: AtomSize(Size::Standard(32)),
                },
                atom_type: AtomType::Udta,
            };

            let res = header.find_child(&mut stream, AtomType::Other([b'd', b'u', b'p', b'e']));

            assert!(
                matches!(&res, Err(ParseError::DuplicateAtom(AtomType::Other(t))) if t == b"dupe"),
                "AtomHeader::find_child() returned an invalid value.\nThe parser should fail when \
                    multiple child 'dupe' atoms are found.\n\nExpected: Err(DuplicateAtom(Other\
                    ([100, 117, 112, 101])))\nGot:      {res:?}",
            );
        }

        #[test]
        fn find_child_returns_error_if_parsing_fails() {
            let mut stream = get_test_udta_stream(|stream| {
                (stream.position() != 8)
                    .then_some(())
                    .ok_or(io::Error::other("udta read err"))
            });

            let header = AtomHeader {
                bounds: AtomBounds {
                    position: 0,
                    size: AtomSize(Size::Standard(32)),
                },
                atom_type: AtomType::Udta,
            };

            let res = header.find_children(&mut stream, AtomType::Other([b's', b'a', b'v', b'o']));

            assert!(
                matches!(
                    res,
                    Err(ParseError::Read(ref e)) if e.kind() == io::ErrorKind::Other
                        && e.to_string() == "udta read err",
                ),
                "AtomHeader::find_child() returned an invalid value.\nThe parser should fail when \
                    the atom content cannot be read.\n\nExpected: Err(Read(Custom {{ kind: Other, \
                    error: \"udta read err\" }}))\nGot:      {res:?}",
            );
        }

        #[test]
        fn find_child_returns_found_atom() {
            let mut stream = get_test_udta_stream(|_| Ok(()));

            let header = AtomHeader {
                bounds: AtomBounds {
                    position: 0,
                    size: AtomSize(Size::Standard(32)),
                },
                atom_type: AtomType::Udta,
            };

            let res = header.find_child(&mut stream, AtomType::Other([b's', b'a', b'v', b'o']));

            assert!(
                matches!(
                    res,
                    Ok(Some(AtomHeader {
                        bounds: AtomBounds {
                            position: 8,
                            size: AtomSize(Size::Standard(8)),
                        },
                        atom_type: AtomType::Other([b's', b'a', b'v', b'o']),
                    })),
                ),
                "AtomHeader::find_child() returned an invalid value.\nThe parser should return a \
                    child 'savo' atom.\n\nExpected: Ok(Some(AtomHeader {{ bounds: AtomBounds {{ \
                    position: 8, size: AtomSize(Standard(8)) }}, atom_size: Other([115, 97, 118, \
                    111])))\nGot:      {res:?}",
            );
        }
    }

    mod get_stream_end {
        use super::MockStream;
        use crate::{ParseError, get_stream_end};
        use mockall::predicate;
        use std::io;

        #[test]
        fn fails_to_get_current_position() {
            let mut stream = MockStream::new();

            stream
                .expect_seek()
                .with(predicate::eq(io::SeekFrom::Current(0)))
                .return_once(|_| Err(io::Error::other("test error")));

            let res = get_stream_end(&mut stream);

            assert!(
                matches!(
                    res,
                    Err(ParseError::CurrentStreamPosition(ref e))
                        if e.kind() == io::ErrorKind::Other && e.to_string() == "test error"),
                "get_stream_end() returned an invalid value.\nThe mocked error should be returned \
                    when unable to get the current position in the stream.\n\nExpected: \
                    Err(CurrentStreamPosition(Custom {{ kind: Other, error: \"test error\" }}))\n\
                    Got:      {res:?}",
            );
        }

        #[test]
        fn fails_to_seek_to_the_end_of_the_stream() {
            let mut stream = MockStream::new();

            stream
                .expect_seek()
                .with(predicate::eq(io::SeekFrom::Current(0)))
                .once()
                .return_once(|_| Ok(2048));

            stream
                .expect_seek()
                .with(predicate::eq(io::SeekFrom::End(0)))
                .once()
                .return_once(|_| Err(io::Error::other("mock error")));

            let res = get_stream_end(&mut stream);

            assert!(
                matches!(
                    res,
                    Err(ParseError::Seek(ref e))
                        if e.kind() == io::ErrorKind::Other && e.to_string() == "mock error"),
                "get_stream_end() returned an invalid value.\nThe mocked error should be returned \
                    when unable to seek to the end of the stream.\n\n\
                    Expected: Err(Seek(Custom {{ kind: Other, error: \"mock error\" }}))\n\
                    Got:      {res:?}",
            );
        }

        #[test]
        fn fails_to_seek_back_to_the_initial_position() {
            let mut stream = MockStream::new();

            stream
                .expect_seek()
                .with(predicate::eq(io::SeekFrom::Current(0)))
                .once()
                .return_once(|_| Ok(4096));

            stream
                .expect_seek()
                .with(predicate::eq(io::SeekFrom::End(0)))
                .once()
                .return_once(|_| Ok(8192));

            stream
                .expect_seek()
                .with(predicate::eq(io::SeekFrom::Start(4096)))
                .once()
                .return_once(|_| Err(io::Error::other("seek error")));

            let res = get_stream_end(&mut stream);

            assert!(
                matches!(
                    res,
                    Err(ParseError::Seek(ref e))
                        if e.kind() == io::ErrorKind::Other && e.to_string() == "seek error"),
                "get_stream_end() returned an invalid value.\nThe mocked error should be returned \
                    when the stream cannot be reset to its initial position.\n\n\
                    Expected: Err(Seek(Custom {{ kind: Other, error: \"seek error\" }}))\n\
                    Got:      {res:?}",
            );
        }

        #[test]
        fn returns_end_of_stream_position() {
            let mut stream = io::Cursor::new([
                12, 34, 21, 55, 244, 53, 12, 45, 66, 21, 24, 55, 95, 43, 91, 242, 153,
            ]);

            stream.set_position(4);

            let res = get_stream_end(&mut stream);

            assert!(
                matches!(res, Ok(17)),
                "get_stream_end() returned an invalid value.\nExpected: Ok(17)\nGot:      {res:?}",
            );
            assert_eq!(4, stream.position(), "");
        }
    }

    mod moov_atom_builder {
        use super::{
            super::{MoovAtom, MoovAtomBuilder, ParseError, TrakAtom},
            AtomBounds, AtomSize, AtomType, MockStream, MvhdAtom, Size, TrakVideAtom,
            get_mock_mvhd_atom, get_mock_trak_vide_atom, get_mock_udta_atom,
        };

        #[test]
        fn build_fails_if_mvhd_is_missing() {
            let builder = MoovAtom::<MockStream>::builder()
                .trak(TrakAtom::Vide(get_mock_trak_vide_atom(1024)))
                .expect("Failed to add 'trak' atom to the MoovAtomBuilder");

            let res = builder.build(
                AtomBounds {
                    position: 16,
                    size: AtomSize(Size::Standard(32784)),
                },
                MockStream::new(),
            );

            assert!(
                matches!(res, Err(ParseError::MissingAtom(AtomType::Mvhd))),
                "MoovAtomBuilder::build() returned an invalid value.\nThe builder should fail \
                    because the child 'mvhd' atom was not set.\n\nExpected: Err(MissingAtom(Mvhd))\
                    \nGot:      {res:?}",
            );
        }

        #[test]
        fn build_fails_if_trak_is_missing() {
            let builder = MoovAtom::<MockStream>::builder()
                .mvhd(get_mock_mvhd_atom(2048))
                .expect("Failed to add 'mvhd' atom to the MoovAtomBuilder");

            let res = builder.build(
                AtomBounds {
                    position: 32,
                    size: AtomSize(Size::Standard(4096)),
                },
                MockStream::new(),
            );

            assert!(
                matches!(res, Err(ParseError::MissingAtom(AtomType::Trak))),
                "MoovAtomBuilder::build() returned an invalid value.\nThe builder should fail \
                    because the child 'trak' atom was not set.\n\nExpected: Err(MissingAtom(Trak))\
                    \nGot:      {res:?}",
            );
        }

        #[test]
        fn build_returns_valid_moov_atom_without_udta() {
            let builder = MoovAtom::<MockStream>::builder();

            let builder = builder
                .mvhd(get_mock_mvhd_atom(64))
                .and_then(|builder| builder.trak(TrakAtom::Vide(get_mock_trak_vide_atom(92))))
                .expect("Failed to add child atoms to the MoovAtomBuilder");

            let res = builder.build(
                AtomBounds {
                    position: 56,
                    size: AtomSize(Size::Standard(8192)),
                },
                MockStream::new(),
            );

            assert!(
                matches!(
                    &res,
                    Ok(MoovAtom {
                        mvhd: MvhdAtom::V0 { .. },
                        trak: TrakVideAtom { .. },
                        _udta: None,
                        _bounds: AtomBounds {
                            position: 56,
                            size: AtomSize(Size::Standard(8192)),
                        },
                        _stream,
                    }),
                ),
                "MoovAtomBuilder::build() returned an invalid value.\nThe builder should return a \
                    valid 'moov' atom structure when provided with a valid configuration.\n\n\
                    Expected: Ok(MoovAtom {{ mvhd: V0 {{ .. }}, trak: TrakVideAtom {{ .. }}, udta: \
                    None, bounds: AtomBounds {{ position: 56, size: AtomSize(Standard(8192)) }} \
                    }})\nGot:      {res:?}",
            );
        }

        #[test]
        fn build_returns_valid_moov_atom_with_udta() {
            let builder = MoovAtom::<MockStream>::builder();

            let builder = builder
                .mvhd(get_mock_mvhd_atom(256))
                .and_then(|builder| builder.trak(TrakAtom::Vide(get_mock_trak_vide_atom(284))))
                .and_then(|builder| builder.udta(get_mock_udta_atom(598)))
                .expect("Failed to add child atoms to the MoovAtomBuilder");

            let res = builder.build(
                AtomBounds {
                    position: 192,
                    size: AtomSize(Size::Standard(28456)),
                },
                MockStream::new(),
            );

            assert!(
                matches!(
                    &res,
                    Ok(MoovAtom {
                        mvhd: MvhdAtom::V0 { .. },
                        trak: TrakVideAtom { .. },
                        _udta: Some(AtomBounds { .. }),
                        _bounds: AtomBounds {
                            position: 192,
                            size: AtomSize(Size::Standard(28456)),
                        },
                        _stream,
                    }),
                ),
                "MoovAtomBuilder::build() returned an invalid value.\nThe builder should return a \
                    valid 'moov' atom structure containing a 'udta' child atom when provided with \
                    a valid configuration.\n\nExpected: Ok(MoovAtom {{ mvhd: V0 {{ .. }}, trak: \
                    TrakVideAtom {{ .. }}, udta: Some(UdtaAtom {{ .. }}), bounds: AtomBounds {{ \
                    position: 193, size: AtomSize(Standard(28456)) }} }})\nGot:      {res:?}",
            );
        }

        #[test]
        fn duplicate_udta_atoms_are_rejected() {
            let builder = MoovAtom::<MockStream>::builder();

            let builder = builder
                .udta(get_mock_udta_atom(8192))
                .expect("Failed to add 'udta' atom to the MoovAtomBuilder");

            let res = builder.udta(get_mock_udta_atom(10240));

            assert!(
                matches!(res, Err(ParseError::DuplicateAtom(AtomType::Udta))),
                "MoovAtomBuilder::udta() returned an invalid value.\nThe builder should return an \
                    error when adding a duplicate 'udta' atom to the MoovAtomBuilder.\n\nExpected: \
                    Err(DuplicateAtom(Udta))\nGot:      {res:?}",
            );
        }

        #[test]
        fn duplicate_trak_atoms_are_rejected() {
            let builder = MoovAtom::<MockStream>::builder();

            let builder = builder
                .trak(TrakAtom::Vide(get_mock_trak_vide_atom(768)))
                .expect("Failed to add 'trak' atom to the MoovAtomBuilder");

            let res = builder.trak(TrakAtom::Vide(get_mock_trak_vide_atom(1536)));

            assert!(
                matches!(res, Err(ParseError::DuplicateAtom(AtomType::Trak))),
                "MoovAtomBuilder::trak() returned an invalid value.\nThe builder should return an \
                    error when adding a duplicate 'trak' atom to the MoovAtomBuilder.\n\nExpected: \
                    Err(DuplicateAtom(Trak))\nGot:      {res:?}",
            );
        }

        #[test]
        fn non_vide_trak_atoms_are_ignored() {
            let builder = MoovAtom::<MockStream>::builder();

            let builder = builder
                .trak(TrakAtom::Other(AtomBounds {
                    position: 284,
                    size: AtomSize(Size::Standard(316)),
                }))
                .expect("Failed to add 'trak' atom to the MoovAtomBuilder");

            assert!(
                matches!(builder, MoovAtomBuilder { trak: None, .. }),
                "MoovAtomBuilder should ignore a non-'vide' 'trak' atoms.\n\nExpected: \n
                    MoovAtomBuilder {{ trak: None }}\nGot:      {builder:?}",
            );
        }

        #[test]
        fn duplicate_mvhd_atoms_are_rejected() {
            let builder = MoovAtom::<MockStream>::builder();

            let builder = builder
                .mvhd(get_mock_mvhd_atom(2304))
                .expect("Failed to add 'mvhd' atom to the MoovAtomBuilder");

            let res = builder.mvhd(get_mock_mvhd_atom(128));

            assert!(
                matches!(res, Err(ParseError::DuplicateAtom(AtomType::Mvhd))),
                "MoovAtomBuilder::build() returned an invalid value.\nThe builder should return an \
                    error when adding a duplicate 'mvhd' atom to the MoovAtomBuilder.\n\nExpected: \
                    Err(DuplicateAtom(Mvhd))\nGot:      {res:?}",
            );
        }
    }

    mod moov_atom {
        use super::{
            super::MoovAtom, super::ParseError, AtomBounds, AtomHeader, AtomSize, AtomType,
            MockStream, MvhdAtom, Size, TestStream, TrakVideAtom, get_mock_mvhd_atom,
            get_mock_trak_vide_atom, with_duplicate_atom, without_atom,
        };
        use std::{io, time};

        #[test]
        fn duration_is_properly_calculated() {
            let moov = MoovAtom {
                _bounds: AtomBounds {
                    position: 256,
                    size: AtomSize(Size::Standard(24873)),
                },
                mvhd: get_mock_mvhd_atom(264),
                trak: get_mock_trak_vide_atom(292),
                _udta: None,
                _stream: MockStream::new(),
            };

            assert_eq!(
                time::Duration::from_secs(15),
                moov.duration(),
                "MoovAtom::duration() returned an invalid media duration.",
            );
        }

        #[test]
        fn resolution_is_properly_calculated() {
            let moov = MoovAtom {
                _bounds: AtomBounds {
                    position: 256,
                    size: AtomSize(Size::Standard(24873)),
                },
                mvhd: get_mock_mvhd_atom(264),
                trak: get_mock_trak_vide_atom(292),
                _udta: None,
                _stream: MockStream::new(),
            };

            assert_eq!(
                (1920, 1080),
                moov.resolution(),
                "MoovAtom::resolution() returned an invalid media resolution.",
            );
        }

        fn get_test_moov_stream<OnRead>(on_read: OnRead) -> impl io::Read + io::Seek
        where
            OnRead: Fn(&mut io::Cursor<Vec<u8>>) -> io::Result<()>,
        {
            TestStream {
                stream: io::Cursor::new(include!("../tests/fixtures/moov.rs")),
                on_read,
            }
        }

        #[test]
        fn parse_from_stream_fails_if_mvhd_parsing_returns_error() {
            let mut stream = get_test_moov_stream(|stream| {
                (stream.position() != 89)
                    .then_some(())
                    .ok_or(io::Error::other("mvhd parse error"))
            });

            let res = MoovAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 73,
                        size: AtomSize(Size::Standard(1281)),
                    },
                    atom_type: AtomType::Moov,
                },
                &mut stream,
            );

            assert!(
                matches!(
                    res,
                    Err(ParseError::Read(ref e)) if e.kind() == io::ErrorKind::Other &&
                        e.to_string() == "mvhd parse error",
                ),
                "MoovAtom::parse_from_stream() returned an invalid value.\nThe parser should \
                    return an error when the 'trak' atom cannot be parsed.\n\nExpected: \
                    Err(Read(Custom {{ kind: Other, error: \"mvhd parse error\" }}))\n\
                    Got:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_fails_if_trak_parsing_returns_error() {
            let mut stream = get_test_moov_stream(|stream| {
                (stream.position() != 197)
                    .then_some(())
                    .ok_or(io::Error::other("trak parse error"))
            });

            let res = MoovAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 73,
                        size: AtomSize(Size::Standard(1281)),
                    },
                    atom_type: AtomType::Moov,
                },
                &mut stream,
            );

            assert!(
                matches!(
                    res,
                    Err(ParseError::Read(ref e)) if e.kind() == io::ErrorKind::Other &&
                        e.to_string() == "trak parse error",
                ),
                "MoovAtom::parse_from_stream() returned an invalid value.\nThe parser should \
                    return an error when the 'trak' atom cannot be parsed.\n\nExpected: \
                    Err(Read(Custom {{ kind: Other, error: \"trak parse error\" }}))\n\
                    Got:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_fails_with_duplicate_mvhd_atoms() {
            let (mut stream, new_size) =
                with_duplicate_atom(get_test_moov_stream(|_| Ok(())), 73, 81);

            let res = MoovAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 73,
                        size: AtomSize(Size::Standard(new_size)),
                    },
                    atom_type: AtomType::Moov,
                },
                &mut stream,
            );

            assert!(
                matches!(res, Err(ParseError::DuplicateAtom(AtomType::Mvhd))),
                "MoovAtom::parse_from_stream() returned an invalid value.\nThe parser should \
                    return an error when a duplicate 'mvhd' atom is provided.\n\nExpected: \
                    Err(DuplicateAtom(Udta))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_fails_with_duplicate_trak_atoms() {
            let (mut stream, new_size) =
                with_duplicate_atom(get_test_moov_stream(|_| Ok(())), 73, 189);

            let res = MoovAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 73,
                        size: AtomSize(Size::Standard(new_size)),
                    },
                    atom_type: AtomType::Moov,
                },
                &mut stream,
            );

            assert!(
                matches!(res, Err(ParseError::DuplicateAtom(AtomType::Trak))),
                "MoovAtom::parse_from_stream() returned an invalid value.\nThe parser should \
                    return an error when a duplicate 'trak' atom is provided.\n\nExpected: \
                    Err(DuplicateAtom(Udta))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_fails_with_duplicate_udta_atoms() {
            let (mut stream, new_size) =
                with_duplicate_atom(get_test_moov_stream(|_| Ok(())), 73, 1167);

            let res = MoovAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 73,
                        size: AtomSize(Size::Standard(new_size)),
                    },
                    atom_type: AtomType::Moov,
                },
                &mut stream,
            );

            assert!(
                matches!(res, Err(ParseError::DuplicateAtom(AtomType::Udta))),
                "MoovAtom::parse_from_stream() returned an invalid value.\nThe parser should \
                    return an error when a duplicate 'udta' atom is provided.\n\nExpected: \
                    Err(DuplicateAtom(Udta))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_fails_if_builder_returns_error() {
            let (mut stream, new_size) = without_atom(get_test_moov_stream(|_| Ok(())), 73, 189);

            let res = MoovAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 73,
                        size: AtomSize(Size::Standard(new_size)),
                    },
                    atom_type: AtomType::Moov,
                },
                &mut stream,
            );

            assert!(
                matches!(res, Err(ParseError::MissingAtom(AtomType::Trak))),
                "MoovAtom::parse_from_stream() returned an invalid value.\nThe parser should fail \
                    when the 'trak' atom is missing from the stream.\n\nExpected: \
                    Err(MissingAtom(Trak))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_valid_stream_returns_moov_atom_with_udta() {
            let mut stream = get_test_moov_stream(|_| Ok(()));

            let res = MoovAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 73,
                        size: AtomSize(Size::Standard(1281)),
                    },
                    atom_type: AtomType::Moov,
                },
                &mut stream,
            );

            assert!(
                matches!(
                    &res,
                    Ok(MoovAtom {
                        _bounds: AtomBounds {
                            position: 73,
                            size: AtomSize(Size::Standard(1281)),
                        },
                        mvhd: MvhdAtom::V0 { .. },
                        trak: TrakVideAtom { .. },
                        _udta: Some(AtomBounds { .. }),
                        _stream,
                    })
                ),
                "Moov::parse_from_stream() returned an invalid value.\nThe parser should return a \
                    valid 'moov' atom when provided with the input stream.\n\nExpected: \
                    Ok(MoovAtom {{ bounds: AtomBounds {{ position: 73, size: \
                        AtomSize(Standard(1281)) }}, mvhd: V0 {{ .. }}, trak: TrakVideAtom {{ .. \
                        }}, udta: Some(UdtaAtom {{ .. }}))\nGot:      {res:?}",
            );
        }

        fn get_test_moov_stream_without_udta<OnRead>(on_read: OnRead) -> impl io::Seek + io::Read
        where
            OnRead: Fn(&mut io::Cursor<Vec<u8>>) -> io::Result<()>,
        {
            let mut moov = include!("../tests/fixtures/moov.rs");
            moov[1171] = b'v';

            TestStream {
                stream: io::Cursor::new(moov),
                on_read,
            }
        }

        #[test]
        fn parse_from_valid_stream_returns_moov_atom() {
            let (mut stream, new_size) =
                without_atom(get_test_moov_stream_without_udta(|_| Ok(())), 73, 1167);

            let res = MoovAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 73,
                        size: AtomSize(Size::Standard(new_size)),
                    },
                    atom_type: AtomType::Moov,
                },
                &mut stream,
            );

            assert!(
                matches!(
                    &res,
                    Ok(MoovAtom {
                        _bounds: AtomBounds {
                            position: 73,
                            size: AtomSize(Size::Standard(1094)),
                        },
                        mvhd: MvhdAtom::V0 { .. },
                        trak: TrakVideAtom { .. },
                        _udta: None,
                        _stream,
                    })
                ),
                "Moov::parse_from_stream() returned an invalid value.\nThe parser should return a \
                    valid 'moov' atom when provided with an input stream missing the child 'udta' \
                    atom.\n\nExpected: Ok(MoovAtom {{ bounds: AtomBounds {{ position: 73, size: \
                    AtomSize(Standard(1281)) }}, mvhd: V0 {{ .. }}, trak: TrakVideAtom {{ .. }}, \
                    udta: Some(UdtaAtom {{ .. }}))\nGot:      {res:?}",
            );
        }
    }

    mod mdia_atom_builder {
        use super::{
            super::ParseError, AtomBounds, AtomSize, AtomType, HdlrAtom, MdhdAtom, MdiaAtom,
            MinfAtom, Size, get_mock_hdlr_vide_atom, get_mock_mdhd_atom, get_mock_minf_atom,
        };

        #[test]
        fn duplicate_hdlr_atoms_are_rejected() {
            let builder = MdiaAtom::builder()
                .hdlr(get_mock_hdlr_vide_atom(192))
                .expect("Failed to add the 'hdlr' atom to the MdiaAtomBuilder");

            let res = builder.hdlr(get_mock_hdlr_vide_atom(384));

            assert!(
                matches!(res, Err(ParseError::DuplicateAtom(AtomType::Hdlr))),
                "MdiaTomBuilder::hdlr() returned an invalid value.\nThe builder should return an \
                    error when a duplicate 'hdlr' atom is added.\n\nExpected: \
                    Err(DuplicateAtom(Hdlr))\nGot:      {res:?}",
            );
        }

        #[test]
        fn duplicate_minf_atoms_are_rejected() {
            let builder = MdiaAtom::builder()
                .minf(get_mock_minf_atom(576))
                .expect("Failed to add the 'minf' atom to the MdiaAtomBuilder");

            let res = builder.minf(get_mock_minf_atom(960));

            assert!(
                matches!(res, Err(ParseError::DuplicateAtom(AtomType::Minf))),
                "MdiaTomBuilder::minf() returned an invalid value.\nThe builder should return an \
                    error when a duplicate 'minf' atom is added.\n\nExpected: \
                    Err(DuplicateAtom(Minf))\nGot:      {res:?}",
            );
        }

        #[test]
        fn duplicate_mdhd_atoms_are_rejected() {
            let builder = MdiaAtom::builder()
                .mdhd(get_mock_mdhd_atom(1984))
                .expect("Failed to add the 'mdhd' atom to the MdiaAtomBuilder");

            let res = builder.mdhd(get_mock_mdhd_atom(2496));

            assert!(
                matches!(res, Err(ParseError::DuplicateAtom(AtomType::Mdhd))),
                "MdiaTomBuilder::mdhd() returned an invalid value.\nThe builder should return an \
                    error when a duplicate 'mdhd' atom is added.\n\nExpected: \
                    Err(DuplicateAtom(Mdhd))\nGot:      {res:?}",
            );
        }

        #[test]
        fn build_fails_if_hdlr_is_missing() {
            let builder = MdiaAtom::builder()
                .mdhd(get_mock_mdhd_atom(3072))
                .and_then(|builder| builder.minf(get_mock_minf_atom(4096)))
                .expect("Failed to add child atoms to the MdiaAtomBuilder");

            let res = builder.build(AtomBounds {
                position: 288,
                size: AtomSize(Size::Standard(4736)),
            });

            assert!(
                matches!(res, Err(ParseError::MissingAtom(AtomType::Hdlr))),
                "MdiaAtomBuilder::build() returned an invalid value.\nThe builder should return an \
                error when the child 'hdlr' atom is missing.\n\nExpected: Err(MissingAtom(Hdlr))\n\
                Got:      {res:?}",
            );
        }

        #[test]
        fn build_fails_if_minf_is_missing() {
            let builder = MdiaAtom::builder()
                .mdhd(get_mock_mdhd_atom(2816))
                .and_then(|builder| builder.hdlr(get_mock_hdlr_vide_atom(3840)))
                .expect("Failed to add child atoms to the MdiaAtomBuilder");

            let res = builder.build(AtomBounds {
                position: 320,
                size: AtomSize(Size::Standard(4800)),
            });

            assert!(
                matches!(res, Err(ParseError::MissingAtom(AtomType::Minf))),
                "MdiaAtomBuilder::build() returned an invalid value.\nThe builder should return an \
                error when the child 'minf' atom is missing.\n\nExpected: Err(MissingAtom(Minf))\n\
                Got:      {res:?}",
            );
        }

        #[test]
        fn build_fails_if_mdhd_is_missing() {
            let builder = MdiaAtom::builder()
                .minf(get_mock_minf_atom(1792))
                .and_then(|builder| builder.hdlr(get_mock_hdlr_vide_atom(3824)))
                .expect("Failed to add child atoms to the MdiaAtomBuilder");

            let res = builder.build(AtomBounds {
                position: 304,
                size: AtomSize(Size::Standard(5504)),
            });

            assert!(
                matches!(res, Err(ParseError::MissingAtom(AtomType::Mdhd))),
                "MdiaAtomBuilder::build() returned an invalid value.\nThe builder should return an \
                error when the child 'mdhd' atom is missing.\n\nExpected: Err(MissingAtom(Mdhd))\n\
                Got:      {res:?}",
            );
        }

        #[test]
        fn build_returns_valid_mdia_atom() {
            let builder = MdiaAtom::builder()
                .minf(get_mock_minf_atom(165))
                .and_then(|builder| builder.hdlr(get_mock_hdlr_vide_atom(1008)))
                .and_then(|builder| builder.mdhd(get_mock_mdhd_atom(1422)))
                .expect("Failed to add child atoms to the MdiaAtomBuilder");

            let res = builder.build(AtomBounds {
                position: 144,
                size: AtomSize(Size::Standard(2004)),
            });

            assert!(
                matches!(
                    res,
                    Ok(MdiaAtom {
                        _bounds: AtomBounds { .. },
                        hdlr: HdlrAtom::Vide { .. },
                        minf: MinfAtom { .. },
                        mdhd: MdhdAtom { .. },
                    }),
                ),
                "MdiaAtomBuilder::build() returned an invalid value.\nThe builder should return a \
                    valid 'mdia' atom structure when provided with a valid configuration.\n\n\
                    Expected: Ok(MdiaAtom{{ bounds: AtomBounds {{ .. }}, hdlr: HdlrAtom::Vide \
                    {{ .. }}, minf: MinfAtom {{ .. }}, MdhdAtom {{ .. }} }}\nGot:     {res:?}",
            );
        }
    }

    mod mdia_atom {
        use super::{
            super::ParseError, AtomBounds, AtomHeader, AtomSize, AtomType, HdlrAtom, MdhdAtom,
            MdiaAtom, MinfAtom, Size, StblAtom, SttsAtom, TestStream, get_mock_mdia_atom,
            with_duplicate_atom, without_atom,
        };
        use std::io;

        fn get_test_mdia_stream<OnRead>(on_read: OnRead) -> impl io::Read + io::Seek
        where
            OnRead: Fn(&mut io::Cursor<Vec<u8>>) -> io::Result<()>,
        {
            TestStream {
                stream: io::Cursor::new(include!("../tests/fixtures/moov.rs")),
                on_read,
            }
        }

        #[test]
        fn parse_from_stream_fails_if_hdlr_parsing_returns_error() {
            let mut stream = get_test_mdia_stream(|stream| {
                (stream.position() != 373)
                    .then_some(())
                    .ok_or(io::Error::other("hdlr read error"))
            });

            let res = MdiaAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 325,
                        size: AtomSize(Size::Standard(842)),
                    },
                    atom_type: AtomType::Mdia,
                },
                &mut stream,
            );

            assert!(
                matches!(
                    res,
                    Err(ParseError::Read(ref e)) if e.kind() == io::ErrorKind::Other
                        && e.to_string() == "hdlr read error",
                ),
                "MdiaAtom::parse_from_stream() returned an invalid value.\nThe parser should fail \
                    when the child 'hdlr' atom cannot be read.\n\nExpected: Err(Read(Custom \
                    {{ kind: Other, error: \"hdlr read error\" }}))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_fails_if_minf_parsing_returns_error() {
            let mut stream = get_test_mdia_stream(|stream| {
                (stream.position() != 418)
                    .then_some(())
                    .ok_or(io::Error::other("minf read error"))
            });

            let res = MdiaAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 325,
                        size: AtomSize(Size::Standard(842)),
                    },
                    atom_type: AtomType::Mdia,
                },
                &mut stream,
            );

            assert!(
                matches!(
                    res,
                    Err(ParseError::Read(ref e)) if e.kind() == io::ErrorKind::Other
                        && e.to_string() == "minf read error",
                ),
                "MdiaAtom::parse_from_stream() returned an invalid value.\nThe parser should fail \
                    when the child 'minf' atom cannot be read.\n\nExpected: Err(Read(Custom \
                    {{ kind: Other, error: \"minf read error\" }}))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_fails_if_mdhd_parsing_returns_error() {
            let mut stream = get_test_mdia_stream(|stream| {
                (stream.position() != 341)
                    .then_some(())
                    .ok_or(io::Error::other("mdhd read error"))
            });

            let res = MdiaAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 325,
                        size: AtomSize(Size::Standard(842)),
                    },
                    atom_type: AtomType::Mdia,
                },
                &mut stream,
            );

            assert!(
                matches!(
                    res,
                    Err(ParseError::Read(ref e)) if e.kind() == io::ErrorKind::Other
                        && e.to_string() == "mdhd read error",
                ),
                "MdiaAtom::parse_from_stream() returned an invalid value.\nThe parser should fail \
                    when the child 'mdhd' atom cannot be read.\n\nExpected: Err(Read(Custom \
                    {{ kind: Other, error: \"mdhd read error\" }}))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_fails_if_builder_returns_error() {
            let (mut stream, size) = without_atom(get_test_mdia_stream(|_| Ok(())), 325, 410);
            let res = MdiaAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 325,
                        size: AtomSize(Size::Standard(size)),
                    },
                    atom_type: AtomType::Mdia,
                },
                &mut stream,
            );

            assert!(
                matches!(res, Err(ParseError::MissingAtom(AtomType::Minf))),
                "MdiaAtom::parse_from_stream() returned an invalid value.\nThe builder should fail \
                    when any of the required child atoms are missing from the stream.\n\nExpected: \
                    Err(MissingAtom(Minf))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_returns_valid_mdia_atom() {
            let mut stream = get_test_mdia_stream(|_| Ok(()));

            let res = MdiaAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 325,
                        size: AtomSize(Size::Standard(842)),
                    },
                    atom_type: AtomType::Mdia,
                },
                &mut stream,
            );

            assert!(
                matches!(
                    res,
                    Ok(MdiaAtom {
                        _bounds: AtomBounds {
                            position: 325,
                            size: AtomSize(Size::Standard(842)),
                        },
                        hdlr: HdlrAtom::Vide(..),
                        minf: MinfAtom { .. },
                        mdhd: MdhdAtom { .. },
                    })
                ),
                "MdiaAtom::parse_from_stream() returned an invalid value.\nThe builder should \
                    return the parsed 'mdia' atom including all of its child atoms.\n\nExpected: \
                    Ok(MdiaAtom {{ bounds: AtomBounds {{ position: 325, size: \
                    AtomSize(Standard(842)) }}, hdlr: Vide(..), minf: MinfAtom {{ .. }}, mdhd: \
                    MdhdAtom {{ .. }} }})\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_fails_with_duplicate_hdlr_atoms() {
            let (mut stream, new_size) =
                with_duplicate_atom(get_test_mdia_stream(|_| Ok(())), 325, 365);

            let res = MdiaAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 325,
                        size: AtomSize(Size::Standard(new_size)),
                    },
                    atom_type: AtomType::Mdia,
                },
                &mut stream,
            );

            assert!(
                matches!(res, Err(ParseError::DuplicateAtom(AtomType::Hdlr))),
                "MdiaAtom::parse_from_stream() returned an invalid value.\nThe parser should fail \
                    when the 'hdlr' atom is missing from the stream.\n\nExpected: \
                    Err(DuplicateAtom(Hdlr))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_fails_with_duplicate_minf_atoms() {
            let (mut stream, new_size) =
                with_duplicate_atom(get_test_mdia_stream(|_| Ok(())), 325, 410);

            let res = MdiaAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 325,
                        size: AtomSize(Size::Standard(new_size)),
                    },
                    atom_type: AtomType::Mdia,
                },
                &mut stream,
            );

            assert!(
                matches!(res, Err(ParseError::DuplicateAtom(AtomType::Minf))),
                "MdiaAtom::parse_from_stream() returned an invalid value.\nThe parser should fail \
                    when the 'minf' atom is missing from the stream.\n\nExpected: \
                    Err(DuplicateAtom(Minf))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_fails_with_duplicate_mdhd_atoms() {
            let (mut stream, new_size) =
                with_duplicate_atom(get_test_mdia_stream(|_| Ok(())), 325, 333);

            let res = MdiaAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 325,
                        size: AtomSize(Size::Standard(new_size)),
                    },
                    atom_type: AtomType::Mdia,
                },
                &mut stream,
            );

            assert!(
                matches!(res, Err(ParseError::DuplicateAtom(AtomType::Mdhd))),
                "MdiaAtom::parse_from_stream() returned an invalid value.\nThe parser should fail \
                    when the 'mdhd' atom is missing from the stream.\n\nExpected: \
                    Err(DuplicateAtom(Mdhd))\nGot:      {res:?}",
            );
        }

        #[test]
        fn fps_returns_calculated_video_fps() {
            let mdia = get_mock_mdia_atom(439);

            let res = mdia.fps();

            assert!(
                matches!(res, Ok(30)),
                "MdiaAtom::fps() returned an invalid value.\nThe framerate calculation failed to \
                produce the correct framerate.\nExpected: Ok(30)\nGot:      {res:?}",
            );
        }

        #[test]
        fn fps_returns_error_if_calculation_fails() {
            let mdia = get_mock_mdia_atom(432);

            let mdia = MdiaAtom {
                mdhd: MdhdAtom {
                    timescale: u32::MAX,
                    ..mdia.mdhd
                },
                minf: MinfAtom {
                    stbl: StblAtom {
                        stts: SttsAtom {
                            table: vec![
                                (u32::MAX, 31),
                                (u32::MAX, 27),
                                (u32::MAX, 17),
                                (u32::MAX, 30),
                                (u32::MAX, 64),
                                (u32::MAX, 51),
                                (u32::MAX, 23),
                                (u32::MAX, 28),
                                (u32::MAX, 24),
                                (u32::MAX, 39),
                            ],
                            ..mdia.minf.stbl.stts
                        },
                        ..mdia.minf.stbl
                    },
                    ..mdia.minf
                },
                ..mdia
            };

            let res = mdia.fps();

            assert!(
                matches!(res, Err(ParseError::MathError(AtomType::Mdia))),
                "MdiaAtom::fps() returned an invalid value.\nThe framerate calculation should \
                    return an error when the calculated values overflow the 64-bit limit.\n\
                    Expected: Err(MathError(Mdia))\nGot:      {res:?}",
            );
        }

        #[test]
        fn fps_returns_error_if_unit_division_fails() {
            let mdia = get_mock_mdia_atom(812);

            let mdia = MdiaAtom {
                mdhd: MdhdAtom {
                    timescale: u32::MAX,
                    ..mdia.mdhd
                },
                minf: MinfAtom {
                    stbl: StblAtom {
                        _bounds: AtomBounds {
                            size: AtomSize(Size::Standard(16)),
                            ..mdia.minf.stbl._bounds
                        },
                        stts: SttsAtom {
                            table: vec![],
                            ..mdia.minf.stbl.stts
                        },
                    },
                    ..mdia.minf
                },
                ..mdia
            };

            let res = mdia.fps();

            assert!(
                matches!(res, Err(ParseError::MathError(AtomType::Mdia))),
                "MdiaAtom::fps() returned an invalid value.\nThe framerate calculation should \
                    return an error when a division by zero occurs.\nExpected: Err(MathError(Mdia))\
                    \nGot:      {res:?}",
            );
        }
    }

    mod trak_atom_builder {
        use super::{
            super::{ParseError, TrakAtom},
            AtomBounds, AtomHeader, AtomSize, AtomType, MdiaAtom, Size, TkhdAtom, TrakVideAtom,
            get_mock_hdlr_other_atom, get_mock_mdhd_atom, get_mock_mdia_atom, get_mock_minf_atom,
            get_mock_tkhd_atom,
        };

        #[test]
        fn duplicate_tkhd_atoms_are_rejected() {
            let builder = TrakAtom::builder()
                .tkhd(get_mock_tkhd_atom(731))
                .expect("Failed to add the child 'tkhd' atom to the TrakAtomBuilder");

            let res = builder.tkhd(get_mock_tkhd_atom(3821));

            assert!(
                matches!(res, Err(ParseError::DuplicateAtom(AtomType::Tkhd))),
                "TrakAtomBuilder::mdia() returned an invalid value.\nThe builder should return an \
                    error when a duplicate 'tkhd' child atom is added.\n\nExpected: \
                    Err(DuplicateAtom(Tkhd))\nGot:      {res:?}",
            );
        }

        #[test]
        fn duplicate_mdia_atoms_are_rejected() {
            let builder = TrakAtom::builder()
                .mdia(get_mock_mdia_atom(832))
                .expect("Failed to add the child 'mdia' atom to the TrakAtomBuilder");

            let res = builder.mdia(get_mock_mdia_atom(1243));

            assert!(
                matches!(res, Err(ParseError::DuplicateAtom(AtomType::Mdia))),
                "TrakAtomBuilder::mdia() returned an invalid value.\nThe builder should return an \
                    error when a duplicate 'mdia' child atom is added.\n\nExpected: \
                    Err(DuplicateAtom(Mdia))\nGot:      {res:?}",
            );
        }

        #[test]
        fn build_fails_if_tkhd_is_missing() {
            let builder = TrakAtom::builder()
                .mdia(get_mock_mdia_atom(832))
                .expect("Failed to add the child 'mdia' atom to the TrakAtomBuilder");

            let res = builder.build(AtomHeader {
                bounds: AtomBounds {
                    position: 482,
                    size: AtomSize(Size::Standard(4932)),
                },
                atom_type: AtomType::Trak,
            });

            assert!(
                matches!(res, Err(ParseError::MissingAtom(AtomType::Tkhd))),
                "TrakAtomBuilder::build() returned an invalid value.\nThe builder should return an \
                    error when the child 'tkhd' atom is missing.\n\nExpected: \
                    Err(MissingAtom(Tkhd))\nGot:      {res:?}",
            );
        }

        #[test]
        fn build_fails_if_mdia_is_missing() {
            let builder = TrakAtom::builder()
                .tkhd(get_mock_tkhd_atom(732))
                .expect("Failed to add the child 'thkd' atom to the TrakAtomBuilder");

            let res = builder.build(AtomHeader {
                bounds: AtomBounds {
                    position: 312,
                    size: AtomSize(Size::Standard(3418)),
                },
                atom_type: AtomType::Trak,
            });

            assert!(
                matches!(res, Err(ParseError::MissingAtom(AtomType::Mdia))),
                "TrakAtomBuilder::build() returned an invalid value.\nThe builder should return an \
                    error when the child 'mdia' atom is missing.\n\nExpected: \
                    Err(MissingAtom(Mdia))\nGot:      {res:?}",
            );
        }

        #[test]
        fn build_returns_valid_trak_vide_atom() {
            let builder = TrakAtom::builder()
                .mdia(get_mock_mdia_atom(382))
                .and_then(|builder| builder.tkhd(get_mock_tkhd_atom(542)))
                .expect("Failed to add the child atoms to the TrakAtomBuilder");

            let res = builder.build(AtomHeader {
                bounds: AtomBounds {
                    position: 283,
                    size: AtomSize(Size::Standard(1072)),
                },
                atom_type: AtomType::Trak,
            });

            assert!(
                matches!(
                    res,
                    Ok(TrakAtom::Vide(TrakVideAtom {
                        _bounds: AtomBounds { .. },
                        mdia: MdiaAtom { .. },
                        tkhd: TkhdAtom { .. }
                    }))
                ),
                "TrakAtomBuilder::build() returned an invalid value.\nThe builder should return a \
                    valid 'trak' atom of type 'vide' when provided with a valid configuration and \
                    a 'vide' 'mdia' atom.\n\nExpected: Ok(Vide(TrakVideAtom {{ bounds: AtomBounds \
                    {{ .. }}, mdia: MdiaAtom {{ .. }}, tkhd: TkhdAtom {{ .. }} }}))\n\
                    Got:      {res:?}",
            );
        }

        #[test]
        fn build_returns_valid_trak_other_atom() {
            let mdia = MdiaAtom {
                _bounds: AtomBounds {
                    position: 82,
                    size: AtomSize(Size::Standard(646)),
                },
                hdlr: get_mock_hdlr_other_atom(90),
                minf: get_mock_minf_atom(218),
                mdhd: get_mock_mdhd_atom(330),
            };

            let builder = TrakAtom::builder()
                .mdia(mdia)
                .and_then(|builder| builder.tkhd(get_mock_tkhd_atom(542)))
                .expect("Failed to add the child atoms to the TrakAtomBuilder");

            let res = builder.build(AtomHeader {
                bounds: AtomBounds {
                    position: 76,
                    size: AtomSize(Size::Standard(1283)),
                },
                atom_type: AtomType::Trak,
            });

            assert!(
                matches!(
                    res,
                    Ok(TrakAtom::Other(AtomBounds {
                        position: 76,
                        size: AtomSize(Size::Standard(1283)),
                    })),
                ),
                "TrakAtomBuilder::build() returned an invalid value.\nThe builder should return a \
                    valid 'trak' atom of a non-'vide' type when provided with a valid \
                    configuration and a non-'vide' 'mdia' atom.\n\nExpected: Ok(Other(AtomHeader \
                    {{ bounds: AtomBounds {{ position: 76, size: AtomSize(Standard(1283)) }}, \
                    atom_type: Trak }}))\nGot:      {res:?}",
            );
        }
    }

    mod mvhd_atom {
        use super::{
            super::ParseError, AtomBounds, AtomSize, AtomType, MvhdAtom, Size, TestStream,
        };
        use std::{io, time};

        fn get_test_mvhd_v0_stream<OnRead>(
            on_read: OnRead,
        ) -> TestStream<OnRead, io::Cursor<[u8; 107]>>
        where
            OnRead: Fn(&mut io::Cursor<[u8; 107]>) -> io::Result<()>,
        {
            let stream = io::Cursor::new([
                0, 0, 0, 0, // version + flags
                0, 0, 0, 0, // creation_time
                0, 0, 0, 0, // modification_time
                0, 0, 3, 232, // timescale
                0, 0, 3, 232, // duration
                // garbage, don't care
                0, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 3,
                210, 116, 114, 97,
            ]);

            TestStream { stream, on_read }
        }

        #[test]
        fn parse_from_stream_fails_on_header_read_error() {
            let mut stream =
                get_test_mvhd_v0_stream(|_| Err(io::Error::other("read header error")));

            let res = MvhdAtom::parse_from_stream(
                AtomBounds {
                    position: 32,
                    size: AtomSize(Size::Standard(108)),
                },
                &mut stream,
            );

            assert!(
                matches!(
                    res,
                    Err(ParseError::Read(ref e)) if e.kind() == io::ErrorKind::Other
                        && e.to_string() == "read header error",
                ),
                "MvhdAtom::parse_from_stream() returned an invalid value.\nThe parser should fail \
                    when the 'mvhd' atom header cannot be read.\n\nExpected: Err(Read(Custom {{ \
                    kind: Other, error: \"read header error\" }}))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_fails_on_invalid_version() {
            let stream = io::Cursor::new([
                255, 0, 0, 0, // version + flags
                0, 0, 0, 0, 0, 0, 0, 0, // creation_time
                0, 0, 0, 0, 0, 0, 0, 0, // modification_time
                0, 0, 3, 232, // timescale
                0, 0, 0, 0, 0, 0, 3, 232, // duration
                // garbage, don't care
                0, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 3,
                210, 116, 114, 97,
            ]);

            let mut stream = TestStream {
                stream,
                on_read: |_| Ok(()),
            };

            let res = MvhdAtom::parse_from_stream(
                AtomBounds {
                    position: 28,
                    size: AtomSize(Size::Standard(108)),
                },
                &mut stream,
            );

            assert!(
                matches!(res, Err(ParseError::AtomVersion(255, AtomType::Mvhd))),
                "MvhdAtom::parse_from_stream() returned an invalid value.\nThe parser should fail \
                    when the 'mvhd' atom contains an invalid version field.\n\nExpected: \
                    Err(AtomVersion(255, Mvhd))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_fails_on_v0_content_read_error() {
            let mut stream = get_test_mvhd_v0_stream(|stream| {
                (stream.position() != 4)
                    .then_some(())
                    .ok_or(io::Error::other("read mvhd body error"))
            });

            let res = MvhdAtom::parse_from_stream(
                AtomBounds {
                    position: 48,
                    size: AtomSize(Size::Standard(108)),
                },
                &mut stream,
            );

            assert!(
                matches!(
                    res,
                    Err(ParseError::Read(ref e)) if e.kind() == io::ErrorKind::Other
                        && e.to_string() == "read mvhd body error",
                ),
                "MvhdAtom::parse_from_stream() returned an invalid value.\nThe parser should fail \
                    when the version 0 'mvhd' atom body cannot be read.\n\nExpected: \
                    Err(Read(Custom {{ kind: Other, error: \"read mvhd body error\" }})\n\
                    Got:      {res:?}",
            );
        }

        fn get_test_mvhd_v1_stream<OnRead>(
            on_read: OnRead,
        ) -> TestStream<OnRead, io::Cursor<[u8; 119]>>
        where
            OnRead: Fn(&mut io::Cursor<[u8; 119]>) -> io::Result<()>,
        {
            let stream = io::Cursor::new([
                1, 0, 0, 0, // version + flags
                0, 0, 0, 0, 0, 0, 0, 0, // creation_time
                0, 0, 0, 0, 0, 0, 0, 0, // modification_time
                0, 0, 3, 232, // timescale
                0, 0, 0, 0, 0, 0, 3, 232, // duration
                // garbage, don't care
                0, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 3,
                210, 116, 114, 97,
            ]);

            TestStream { stream, on_read }
        }

        #[test]
        fn parse_from_stream_fails_on_v1_content_read_error() {
            let mut stream = get_test_mvhd_v1_stream(|stream| {
                (stream.position() != 4)
                    .then_some(())
                    .ok_or(io::Error::other("mvhd body read err"))
            });

            let res = MvhdAtom::parse_from_stream(
                AtomBounds {
                    position: 74,
                    size: AtomSize(Size::Standard(120)),
                },
                &mut stream,
            );

            assert!(
                matches!(
                    res,
                    Err(ParseError::Read(ref e)) if e.kind() == io::ErrorKind::Other
                        && e.to_string() == "mvhd body read err",
                ),
                "MvhdAtom::parse_from_stream() returned an invalid value.\nThe parser should fail \
                    when the version 1 'mvhd' atom body cannot be read.\n\nExpected: \
                    Err(Read(Custom {{ kind: Other, error: \"mvhd body read err\" }})\n\
                    Got:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_returns_valid_mvhd_v0_atom() {
            let mut stream = get_test_mvhd_v0_stream(|_| Ok(()));

            let res = MvhdAtom::parse_from_stream(
                AtomBounds {
                    position: 48,
                    size: AtomSize(Size::Standard(108)),
                },
                &mut stream,
            );

            assert!(
                matches!(
                    res,
                    Ok(MvhdAtom::V0 {
                        _bounds: AtomBounds {
                            position: 48,
                            size: AtomSize(Size::Standard(108)),
                        },
                        timescale: 1000,
                        duration: 1000,
                    }),
                ),
                "MvhdAtom::parse_from_stream() returned an invalid value.\nThe parser should \
                    return a valid version 0 'mvhd' atom when provided with a valid input stream.\n\
                    \nExpected: Ok(V0 {{ bounds: AtomBounds {{ position: 48, size: \
                    AtomSize(Standard(108)) }}, timescale: 1000, duration: 1000 }}\n\
                    Got:      {res:?}",
            );

            assert_eq!(
                time::Duration::from_secs(1),
                res.unwrap().duration(),
                "MvhdAtom::duration() returned invalid media duration."
            );
        }

        #[test]
        fn parse_from_stream_returns_valid_mvhd_v1_atom() {
            let mut stream = get_test_mvhd_v1_stream(|_| Ok(()));

            let res = MvhdAtom::parse_from_stream(
                AtomBounds {
                    position: 23,
                    size: AtomSize(Size::Standard(120)),
                },
                &mut stream,
            );

            assert!(
                matches!(
                    res,
                    Ok(MvhdAtom::V1 {
                        _bounds: AtomBounds {
                            position: 23,
                            size: AtomSize(Size::Standard(120)),
                        },
                        timescale: 1000,
                        duration: 1000,
                    }),
                ),
                "MvhdAtom::parse_from_stream() returned an invalid value.\nThe parser should \
                    return a valid version 1 'mvhd' atom when provided with a valid input stream.\n\n\
                    Expected: Ok(V1 {{ bounds: AtomBounds {{ position: 23, size: \
                    AtomSize(Standard(120)) }}, timescale: 1000, duration: 1000 }}\n\
                    Got:      {res:?}",
            );

            assert_eq!(
                time::Duration::from_secs(1),
                res.unwrap().duration(),
                "MvhdAtom::duration() returned invalid media duration."
            );
        }
    }

    mod trak_atom {
        use super::{
            super::{ParseError, TrakAtom},
            AtomBounds, AtomHeader, AtomSize, AtomType, MdiaAtom, Size, TestStream, TkhdAtom,
            TrakVideAtom, with_duplicate_atom, without_atom,
        };
        use std::io;

        fn get_test_trak_vide_stream<OnRead>(
            on_read: OnRead,
        ) -> TestStream<OnRead, io::Cursor<Vec<u8>>>
        where
            OnRead: Fn(&mut io::Cursor<Vec<u8>>) -> io::Result<()>,
        {
            let moov = include!("../tests/fixtures/moov.rs");

            let trak: Vec<u8> = moov.into_iter().skip(73).collect();

            TestStream {
                on_read,
                stream: io::Cursor::new(trak),
            }
        }

        #[test]
        fn parse_from_stream_fails_if_mdia_parsing_returns_error() {
            let mut stream = get_test_trak_vide_stream(|stream| {
                (stream.position() != 292)
                    .then_some(())
                    .ok_or(io::Error::other("parse mdia error"))
            });

            let res = TrakAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 116,
                        size: AtomSize(Size::Standard(978)),
                    },
                    atom_type: AtomType::Trak,
                },
                &mut stream,
            );

            assert!(
                matches!(
                    res,
                    Err(ParseError::Read(ref e)) if e.kind() == io::ErrorKind::Other
                        && e.to_string() == "parse mdia error",
                ),
                "TrakAtom::parse_from_stream() returned an invalid value.\nParsing of the 'trak' \
                    atom should fail when the child 'mdia' atom cannot be parsed.\n\nExpected: \
                    Err(Read(Custom {{ kind: Other, error: \"parse mdia error\" }}))\n\
                    Got:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_fails_with_duplicate_mdia_atoms() {
            let (mut stream, new_size) =
                with_duplicate_atom(get_test_trak_vide_stream(|_| Ok(())), 116, 252);

            let res = TrakAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 116,
                        size: AtomSize(Size::Standard(new_size)),
                    },
                    atom_type: AtomType::Trak,
                },
                &mut stream,
            );

            assert!(
                matches!(res, Err(ParseError::DuplicateAtom(AtomType::Mdia))),
                "TrakAtom::parse_from_stream() returned an invalid value.\nThe parser should fail \
                    when provided with a stream containing duplicate 'mdia' atoms.\n\nExpected: \
                    Err(DuplicateAtom(Mdia))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_fails_if_tkhd_parsing_returns_error() {
            let mut stream = get_test_trak_vide_stream(|stream| {
                (stream.position() != 132)
                    .then_some(())
                    .ok_or(io::Error::other("tkhd parse err"))
            });

            let res = TrakAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 116,
                        size: AtomSize(Size::Standard(978)),
                    },
                    atom_type: AtomType::Trak,
                },
                &mut stream,
            );

            assert!(
                matches!(
                    res,
                    Err(ParseError::Read(ref e)) if e.kind() == io::ErrorKind::Other
                        && e.to_string() == "tkhd parse err",
                ),
                "TrakAtom::parse_from_stream() returned an invalid value.\nParsing of the 'trak' \
                    atom should fail when the child 'tkhd' atom cannot be parsed.\n\nExpected: \
                    Err(Read(Custom {{ kind: Other, error: \"tkhd parse err\" }}))\n\
                    Got:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_fails_with_duplicate_tkhd_atoms() {
            let (mut stream, new_size) =
                with_duplicate_atom(get_test_trak_vide_stream(|_| Ok(())), 116, 124);

            let res = TrakAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 116,
                        size: AtomSize(Size::Standard(new_size)),
                    },
                    atom_type: AtomType::Trak,
                },
                &mut stream,
            );

            assert!(
                matches!(res, Err(ParseError::DuplicateAtom(AtomType::Tkhd))),
                "TrakAtom::parse_from_stream() returned an invalid value.\nThe parser should fail \
                    when provided with a stream containing duplicate 'tkhd' atoms.\n\nExpected: \
                    Err(DuplicateAtom(Tkhd))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_fails_if_builder_returns_error() {
            let (mut stream, new_size) =
                without_atom(get_test_trak_vide_stream(|_| Ok(())), 116, 124);

            let res = TrakAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 116,
                        size: AtomSize(Size::Standard(new_size)),
                    },
                    atom_type: AtomType::Trak,
                },
                &mut stream,
            );

            assert!(
                matches!(res, Err(ParseError::MissingAtom(AtomType::Tkhd))),
                "TrakAtom::parse_from_stream() returned an invalid value.\nThe parser should fail \
                    when the builder returns an error because the child 'tkhd' atom is missing.\n\n\
                    Expected: Err(MissingAtom(Tkhd))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_returns_valid_trak_vide_atom() {
            let mut stream = get_test_trak_vide_stream(|_| Ok(()));

            let res = TrakAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 116,
                        size: AtomSize(Size::Standard(978)),
                    },
                    atom_type: AtomType::Trak,
                },
                &mut stream,
            );

            assert!(
                matches!(
                    res,
                    Ok(TrakAtom::Vide(TrakVideAtom {
                        _bounds: AtomBounds {
                            position: 116,
                            size: AtomSize(Size::Standard(978)),
                        },
                        mdia: MdiaAtom { .. },
                        tkhd: TkhdAtom { .. },
                    }))
                ),
                "TrakAtom::parse_from_stream() returned an invalid value.\nThe parser should \
                    return a valid 'trak' atom of type 'vide' when provided with a valid input \
                    stream.\n\nExpected: Ok(Vide(TrakVideAtom {{ bounds: AtomBounds {{ position: \
                    116, size: AtomSize(Standard(978)) }}, mdia: MdiaAtom: {{ .. }}, tkhd: \
                    TkhdAtom {{ .. }} }}))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_returns_valid_trak_other_atom() {
            let moov = include!("../tests/fixtures/moov.rs");

            let mut trak: Vec<u8> = moov.into_iter().skip(73).collect();

            trak[308] = b'a';

            let mut stream = TestStream {
                on_read: |_| Ok(()),
                stream: io::Cursor::new(trak),
            };

            let res = TrakAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 116,
                        size: AtomSize(Size::Standard(978)),
                    },
                    atom_type: AtomType::Trak,
                },
                &mut stream,
            );

            assert!(
                matches!(
                    res,
                    Ok(TrakAtom::Other(AtomBounds {
                        position: 116,
                        size: AtomSize(Size::Standard(978)),
                    }))
                ),
                "TrakAtom::parse_from_stream() returned an invalid value.\nThe parser should \
                    return a valid non-video 'trak' atom when provided with a valid input stream.\
                    \n\nExpected: Ok(Other(AtomBounds {{ position: 116, size: \
                    AtomSize(Standard(978)) }}))\nGot:      {res:?}",
            );
        }
    }

    mod tkhd_atom {
        use super::{
            super::ParseError, AtomBounds, AtomSize, AtomType, MockStream, Size, TestStream,
            TkhdAtom,
        };
        use mockall::predicate;
        use std::io::{self, Seek as _, Write};

        fn get_mock_tkhd_v0_stream<OnRead>(on_read: OnRead) -> impl io::Seek + io::Read
        where
            OnRead: Fn(&mut io::Cursor<Vec<u8>>) -> io::Result<()>,
        {
            let stream = include!("../tests/fixtures/moov.rs")
                .into_iter()
                .skip(73)
                .collect::<Vec<u8>>();

            let mut stream = io::Cursor::new(stream);

            stream
                .seek(io::SeekFrom::Start(132))
                .expect("Failed to seek the test stream");

            TestStream { stream, on_read }
        }

        #[test]
        fn parse_from_stream_fails_if_unable_to_read_header() {
            let mut stream = get_mock_tkhd_v0_stream(|stream| {
                (stream.position() != 132)
                    .then_some(())
                    .ok_or(io::Error::other("tkhd header read err"))
            });

            let res = TkhdAtom::parse_from_stream(
                AtomBounds {
                    position: 124,
                    size: AtomSize(Size::Standard(92)),
                },
                &mut stream,
            );

            assert!(
                matches!(
                    res,
                    Err(ParseError::Read(ref e)) if e.kind() == io::ErrorKind::Other
                        && e.to_string() == "tkhd header read err",
                ),
                "TkhdAtom::parse_from_stream() returned an invalid value.\nThe parser should fail \
                    when the atom header cannot be read.\n\nExpected: Err(Read(Custom {{ kind: \
                    Other, error: \"tkhd header read err\" }}))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_fails_on_invalid_version() {
            let mut stream = include!("../tests/fixtures/moov.rs")
                .into_iter()
                .skip(205)
                .collect::<Vec<u8>>();
            stream[0] = 255;

            let mut stream = io::Cursor::new(stream);

            let res = TkhdAtom::parse_from_stream(
                AtomBounds {
                    position: 124,
                    size: AtomSize(Size::Standard(92)),
                },
                &mut stream,
            );

            assert!(
                matches!(res, Err(ParseError::AtomVersion(255, AtomType::Tkhd))),
                "TkhdAtom::parse_from_stream() returned an invalid value.\nThe parser should fail \
                    when the 'tkhd' atom contains an invalid version field.\n\nExpected: \
                    Err(AtomVersion(255, Tkhd))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_fails_on_seek_error() {
            let mut stream = MockStream::new();
            stream
                .expect_read()
                .return_once(|mut buf| buf.write(&[0, 0, 0, 0]));
            stream
                .expect_seek()
                .with(predicate::eq(io::SeekFrom::Current(72)))
                .return_once(|_| Err(io::Error::other("tkhd seek err")));

            let res = TkhdAtom::parse_from_stream(
                AtomBounds {
                    position: 124,
                    size: AtomSize(Size::Standard(92)),
                },
                &mut stream,
            );

            assert!(
                matches!(
                    res,
                    Err(ParseError::Seek(ref e)) if e.kind() == io::ErrorKind::Other
                        && e.to_string() == "tkhd seek err",
                ),
                "TkhdAtom::parse_from_stream() returned an invalid value.\nThe parser should fail \
                    when the stream cannot be seeked.\n\nExpected: Err(Seek(Custom {{ kind: \
                    Other, error: \"tkhd seek err\" }}))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_fails_on_content_read_error() {
            let mut stream = get_mock_tkhd_v0_stream(|stream| {
                (stream.position() != 208)
                    .then_some(())
                    .ok_or(io::Error::other("tkhd fields read err"))
            });

            let res = TkhdAtom::parse_from_stream(
                AtomBounds {
                    position: 124,
                    size: AtomSize(Size::Standard(92)),
                },
                &mut stream,
            );

            assert!(
                matches!(
                    res,
                    Err(ParseError::Read(ref e)) if e.kind() == io::ErrorKind::Other
                        && e.to_string() == "tkhd fields read err",
                ),
                "TkhdAtom::parse_from_stream() returned an invalid value.\nThe parser should fail \
                    when the atom fields cannot be read.\n\nExpected: Err(Read(Custom {{ kind: \
                    Other, error: \"tkhd fields read err\" }}))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_returns_valid_tkhd_v0_atom() {
            let mut stream = get_mock_tkhd_v0_stream(|_| Ok(()));

            let res = TkhdAtom::parse_from_stream(
                AtomBounds {
                    position: 124,
                    size: AtomSize(Size::Standard(92)),
                },
                &mut stream,
            );

            assert!(
                matches!(
                    res,
                    Ok(TkhdAtom {
                        _bounds: AtomBounds {
                            position: 124,
                            size: AtomSize(Size::Standard(92)),
                        },
                        width: 640,
                        height: 360,
                    }),
                ),
                "TkhdAtom::parse_from_stream() returned an invalid value.\nThe parser should \
                    return a valid version 0 'tkhd' atom when provided with a valid input stream.\
                    \n\nExpected: Ok(TkhdAtom {{ bounds: AtomBounds {{ position: 124, size: \
                    AtomSize(Standard(92)) }}, width: 640, height: 360 }})\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_returns_valid_tkhd_v1_atom() {
            let mut stream = io::Cursor::new([
                1, 0, 0, 3, // version + fields
                0, 0, 0, 0, 0, 0, 0, 0, // creation_time
                0, 0, 0, 0, 0, 0, 0, 0, // modification_time
                0, 0, 0, 1, // track_id
                0, 0, 0, 0, // reserved
                0, 0, 0, 0, 0, 0, 3, 232, // duration
                0, 0, 0, 0, // reserved
                0, 0, 0, 0, // reserved
                0, 0, // layer
                0, 0, // alterntate_group
                0, 0, // volume
                0, 0, // reserved
                0, 1, 0, 0, // matrix
                0, 0, 0, 0, // matrix
                0, 0, 0, 0, // matrix
                0, 0, 0, 0, // matrix
                0, 1, 0, 0, //matrix
                0, 0, 0, 0, //matrix
                0, 0, 0, 0, //matrix
                0, 0, 0, 0, //matrix
                64, 0, 0, 0, //matrix
                2, 128, 0, 0, // width
                1, 104, 0, 0, // height
            ]);

            let res = TkhdAtom::parse_from_stream(
                AtomBounds {
                    position: 124,
                    size: AtomSize(Size::Standard(104)),
                },
                &mut stream,
            );

            assert!(
                matches!(
                    res,
                    Ok(TkhdAtom {
                        _bounds: AtomBounds {
                            position: 124,
                            size: AtomSize(Size::Standard(104)),
                        },
                        width: 640,
                        height: 360,
                    }),
                ),
                "TkhdAtom::parse_from_stream() returned an invalid value.\nThe parser should \
                    return a valid version 1 'tkhd' atom when provided with a valid input stream.\
                    \n\nExpected: Ok(TkhdAtom {{ bounds: AtomBounds {{ position: 124, size: \
                    AtomSize(Standard(104)) }}, width: 640, height: 360 }})\nGot:      {res:?}",
            );
        }
    }

    mod hdlr_atom {
        use super::{
            super::ParseError, AtomBounds, AtomHeader, AtomSize, AtomType, HdlrAtom, Size,
            TestStream,
        };
        use std::io::{self, Seek as _};

        fn get_test_hdlr_v0_stream<OnRead>(on_read: OnRead) -> impl io::Read + io::Seek
        where
            OnRead: Fn(&mut io::Cursor<Vec<u8>>) -> io::Result<()>,
        {
            let mut stream = io::Cursor::new(vec![
                0, 0, 0, 45, // size
                104, 100, 108, 114, // type: hdlr
                0, 0, 0, 0, // version + flags
                0, 0, 0, 0, // predefined
                118, 105, 100, 101, // handler type: vide
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // reserved
                // name: VideoHandler (null-terminated)
                86, 105, 100, 101, 111, 72, 97, 110, 100, 108, 101, 114, 0,
            ]);

            stream
                .seek(io::SeekFrom::Start(8))
                .expect("Failed to seek the test stream");

            TestStream { stream, on_read }
        }

        #[test]
        fn parse_from_stream_fails_if_unable_to_read_header() {
            let mut stream = get_test_hdlr_v0_stream(|stream| {
                (stream.position() != 8)
                    .then_some(())
                    .ok_or(io::Error::other("hdlr head read err"))
            });

            let res = HdlrAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 247,
                        size: AtomSize(Size::Standard(45)),
                    },
                    atom_type: AtomType::Hdlr,
                },
                &mut stream,
            );

            assert!(
                matches!(
                    res,
                    Err(ParseError::Read(ref e)) if e.kind() == io::ErrorKind::Other
                        && e.to_string() == "hdlr head read err",
                ),
                "HdlrAtom::parse_from_stream() returned an invalid value.\nThe parser should fail \
                    when the 'hdlr' atom header cannot be read from the stream.\n\nExpected: \
                    Err(Read(Custom {{ kind: Other, error: \"hdlr head read err\" }}))\n\
                    Got:      {res:?}",
            )
        }

        #[test]
        fn parse_from_stream_fails_on_invalid_version() {
            let mut stream = io::Cursor::new(vec![
                0, 0, 0, 45, // size
                104, 100, 108, 114, // type: hdlr
                255, 0, 0, 0, // version + flags
                0, 0, 0, 0, // predefined
                118, 105, 100, 101, // handler type: vide
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // reserved
                // name: VideoHandler (null-terminated)
                86, 105, 100, 101, 111, 72, 97, 110, 100, 108, 101, 114, 0,
            ]);

            stream
                .seek(io::SeekFrom::Start(8))
                .expect("Failed to seek the test stream");

            let res = HdlrAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 483,
                        size: AtomSize(Size::Standard(45)),
                    },
                    atom_type: AtomType::Hdlr,
                },
                &mut stream,
            );

            assert!(
                matches!(res, Err(ParseError::AtomVersion(255, AtomType::Hdlr))),
                "HdlrAtom::parse_from_stream() returned an invalid value.\nThe parser should fail \
                    when the 'hdlr' atom contains an invalid version field.\n\nExpected: \
                    Err(AtomVersion(255, Hdlr))\nGot:      {res:?}",
            )
        }

        #[test]
        fn parse_from_stream_fails_if_unable_to_read_type_field() {
            let mut stream = get_test_hdlr_v0_stream(|stream| {
                (stream.position() != 12)
                    .then_some(())
                    .ok_or(io::Error::other("hdlr type read err"))
            });

            let res = HdlrAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 381,
                        size: AtomSize(Size::Standard(45)),
                    },
                    atom_type: AtomType::Hdlr,
                },
                &mut stream,
            );

            assert!(
                matches!(
                    res,
                    Err(ParseError::Read(ref e)) if e.kind() == io::ErrorKind::Other
                        && e.to_string() == "hdlr type read err",
                ),
                "HdlrAtom::parse_from_stream() returned an invalid value.\nThe parser should fail \
                    when the 'hdlr' atom handler field cannot be read from the stream.\n\n\
                    Expected: Err(Read(Custom {{ kind: Other, error: \"hdlr type read err\" }}))\n\
                    Got:      {res:?}",
            )
        }

        #[test]
        fn parse_from_stream_returns_valid_hdlr_vide_atom() {
            let mut stream = get_test_hdlr_v0_stream(|_| Ok(()));

            let res = HdlrAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 381,
                        size: AtomSize(Size::Standard(45)),
                    },
                    atom_type: AtomType::Hdlr,
                },
                &mut stream,
            );

            assert!(
                matches!(
                    res,
                    Ok(HdlrAtom::Vide(AtomBounds {
                        position: 381,
                        size: AtomSize(Size::Standard(45)),
                    })),
                ),
                "HdlrAtom::parse_from_stream() returned an invalid value.\nThe parser should \
                    return a valid 'hdlr' atom of type 'vide' when provided with a valid input \
                    stream.\n\nExpected: Ok(Vide(AtomBounds {{ position: 381, size: \
                    AtomSize(Standard(45)) }}))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_returns_valid_hdlr_other_atom() {
            let mut stream = io::Cursor::new(vec![
                0, 0, 0, 45, // size
                104, 100, 108, 114, // type: hdlr
                0, 0, 0, 0, // version + flags
                0, 0, 0, 0, // predefined
                115, 111, 117, 110, // handler type: soun
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // reserved
                // name: SoundHandler (null-terminated)
                83, 111, 117, 110, 100, 72, 97, 110, 100, 108, 101, 114, 0,
            ]);

            stream
                .seek(io::SeekFrom::Start(8))
                .expect("Failed to seek the test stream");

            let res = HdlrAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 483,
                        size: AtomSize(Size::Standard(45)),
                    },
                    atom_type: AtomType::Hdlr,
                },
                &mut stream,
            );

            assert!(
                matches!(
                    res,
                    Ok(HdlrAtom::Other(AtomBounds {
                        position: 483,
                        size: AtomSize(Size::Standard(45)),
                    })),
                ),
                "HdlrAtom::parse_from_stream() returned an invalid value.\nThe parser should \
                    return a valid 'hdlr' atom of other type when provided with a valid input \
                    stream.\n\nExpected: Ok(Other(AtomBounds {{ position: 483, size: \
                    AtomSize(Standard(45)) }}))\nGot:      {res:?}",
            );
        }
    }

    mod minf_atom {
        use super::{
            super::ParseError, AtomBounds, AtomHeader, AtomSize, AtomType, MinfAtom, Size,
            StblAtom, TestStream, with_duplicate_atom, without_atom,
        };
        use std::io;

        fn get_test_minf_stream<OnRead>(on_read: OnRead) -> impl io::Read + io::Seek
        where
            OnRead: Fn(&mut io::Cursor<Vec<u8>>) -> io::Result<()>,
        {
            let stream = io::Cursor::new(
                include!("../tests/fixtures/moov.rs")
                    .into_iter()
                    .skip(400)
                    .take(757)
                    .collect(),
            );

            TestStream { stream, on_read }
        }
        #[test]
        fn parse_from_stream_fails_with_duplicate_stbl_atoms() {
            let (mut stream, new_size) =
                with_duplicate_atom(get_test_minf_stream(|_| Ok(())), 10, 64);

            let res = MinfAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 10,
                        size: AtomSize(Size::Standard(new_size)),
                    },
                    atom_type: AtomType::Minf,
                },
                &mut stream,
            );

            assert!(
                matches!(res, Err(ParseError::DuplicateAtom(AtomType::Stbl))),
                "MinfAtom::parse_from_stream() returned an invalid value.\nThe parser should \
                    fail when a duplicate 'stbl' atom is provided.\n\nExpected: \
                    Err(DuplicateAtom(Stbl))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_fails_if_stbl_parsing_returns_error() {
            let mut stream = get_test_minf_stream(|stream| {
                (stream.position() != 82)
                    .then_some(())
                    .ok_or(io::Error::other("stbl parse error"))
            });

            let res = MinfAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 10,
                        size: AtomSize(Size::Standard(757)),
                    },
                    atom_type: AtomType::Minf,
                },
                &mut stream,
            );

            assert!(
                matches!(
                    res,
                    Err(ParseError::Read(ref e)) if e.kind() == io::ErrorKind::Other
                        && e.to_string() == "stbl parse error",
                ),
                "MinfAtom::parse_from_stream() returned an invalid value.\nThe parser should \
                    fail when the 'stbl' atom cannot be parsed.\n\nExpected: Err(Read(Custom {{ \
                    kind: Other, error: \"stbl parse error\" }}))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_fails_if_stbl_atom_is_missing() {
            let (mut stream, new_size) = without_atom(get_test_minf_stream(|_| Ok(())), 10, 74);

            let res = MinfAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 10,
                        size: AtomSize(Size::Standard(new_size)),
                    },
                    atom_type: AtomType::Minf,
                },
                &mut stream,
            );

            assert!(
                matches!(res, Err(ParseError::MissingAtom(AtomType::Stbl))),
                "MinfAtom::parse_from_stream() returned an invalid value.\nThe parser should fail \
                    when the 'stbl' atom is missing from the input stream.\n\nExpected: \
                    Err(MissingAtom(Stbl))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_returns_valid_minf_atom() {
            let mut stream = get_test_minf_stream(|_| Ok(()));

            let res = MinfAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 10,
                        size: AtomSize(Size::Standard(757)),
                    },
                    atom_type: AtomType::Minf,
                },
                &mut stream,
            );

            assert!(
                matches!(
                    res,
                    Ok(MinfAtom {
                        _bounds: AtomBounds {
                            position: 10,
                            size: AtomSize(Size::Standard(757)),
                        },
                        stbl: StblAtom { .. },
                    })
                ),
                "MinfAtom::parse_from_stream() returned an invalid value.\nThe parser should \
                    return a valid 'minf' atom structure when provided with a valid input stream.\n\
                    \nExpected: Ok(MinfAtom {{ bounds: AtomBounds {{ position: 10, size: \
                    AtomSize(Standard(757)) }}, stbl: StblAtom {{ .. }} }})\nGot:      {res:?}",
            );
        }
    }

    mod stbl_atom {
        use super::{
            super::ParseError, AtomBounds, AtomHeader, AtomSize, AtomType, Size, StblAtom,
            SttsAtom, TestStream, with_duplicate_atom, without_atom,
        };
        use std::io;

        fn get_test_stbl_stream<OnRead>(on_read: OnRead) -> impl io::Read + io::Seek
        where
            OnRead: Fn(&mut io::Cursor<Vec<u8>>) -> io::Result<()>,
        {
            let stream = io::Cursor::new(
                include!("../tests/fixtures/moov.rs")
                    .into_iter()
                    .skip(459)
                    .take(708)
                    .collect(),
            );

            TestStream { stream, on_read }
        }

        #[test]
        fn parse_from_stream_fails_if_stts_parsing_returns_error() {
            let mut stream = get_test_stbl_stream(|stream| {
                (stream.position() != 228)
                    .then_some(())
                    .ok_or(io::Error::other("stts parse err"))
            });

            let res = StblAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 15,
                        size: AtomSize(Size::Standard(693)),
                    },
                    atom_type: AtomType::Stbl,
                },
                &mut stream,
            );

            assert!(
                matches!(
                    res, Err(ParseError::Read(ref e)) if e.kind() == io::ErrorKind::Other
                        && e.to_string() == "stts parse err",
                ),
                "StblAtom::parse_from_stream() returned an invalid value.\nThe parser should \
                    fail when the 'stts' atom cannot be parsed.\n\nExpected: Err(Read(Custom {{ \
                    kind: Other, error: \"stts parse err\" }}))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_fails_with_duplicate_stts_atoms() {
            let (mut stream, new_size) =
                with_duplicate_atom(get_test_stbl_stream(|_| Ok(())), 15, 220);

            let res = StblAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 15,
                        size: AtomSize(Size::Standard(new_size)),
                    },
                    atom_type: AtomType::Stbl,
                },
                &mut stream,
            );

            assert!(
                matches!(res, Err(ParseError::DuplicateAtom(AtomType::Stts))),
                "StblAtom::parse_from_stream() returned an invalid value.\nThe parser should \
                    fail when the input stream contains duplicate 'stts' atoms.\n\nExpected: \
                    Err(DuplicateAtom(Stts))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_fails_if_builder_returns_error() {
            let (mut stream, new_size) = without_atom(get_test_stbl_stream(|_| Ok(())), 15, 220);

            let res = StblAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 15,
                        size: AtomSize(Size::Standard(new_size)),
                    },
                    atom_type: AtomType::Stbl,
                },
                &mut stream,
            );

            assert!(
                matches!(res, Err(ParseError::MissingAtom(AtomType::Stts))),
                "StblAtom::parse_from_stream() returned an invalid value.\nThe parser should \
                    fail when the 'stts' atom is missing from the input stream.\n\nExpected: \
                    Err(MissingAtom(Stts))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_returns_valid_stbl_atom() {
            let mut stream = get_test_stbl_stream(|_| Ok(()));

            let res = StblAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 15,
                        size: AtomSize(Size::Standard(693)),
                    },
                    atom_type: AtomType::Stbl,
                },
                &mut stream,
            );

            assert!(
                matches!(
                    res,
                    Ok(StblAtom {
                        _bounds: AtomBounds {
                            position: 15,
                            size: AtomSize(Size::Standard(693)),
                        },
                        stts: SttsAtom { .. },
                    })
                ),
                "StblAtom::parse_from_stream() returned an invalid value.\nThe parser should \
                    return a valid 'stbl' atom when provided with a valid input stream.\n\n\
                    Expected: Ok(StblAtom {{ bounds: AtomBounds {{ position: 15, size: \
                    AtomSize(Standard(693)) }}, stts: SttsAtom {{ .. }})\nGot:      {res:?}",
            );
        }
    }

    mod stts_atom {
        use super::{
            super::ParseError, AtomBounds, AtomSize, AtomType, Size, SttsAtom, TestStream,
        };
        use std::io::{self, Seek as _};

        fn get_test_stts_stream<OnRead>(on_read: OnRead) -> impl io::Read + io::Seek
        where
            OnRead: Fn(&mut io::Cursor<Vec<u8>>) -> io::Result<()>,
        {
            let mut stream = io::Cursor::new(
                include!("../tests/fixtures/moov.rs")
                    .into_iter()
                    .skip(662)
                    .take(41)
                    .collect(),
            );

            stream
                .seek(io::SeekFrom::Start(25))
                .expect("Failed to seek the test stream");

            TestStream { stream, on_read }
        }

        #[test]
        fn parse_from_stream_fails_if_unable_to_read_header() {
            let mut stream = get_test_stts_stream(|stream| {
                (stream.position() != 25)
                    .then_some(())
                    .ok_or(io::Error::other("stts header read err"))
            });

            let res = SttsAtom::parse_from_stream(
                AtomBounds {
                    position: 17,
                    size: AtomSize(Size::Standard(24)),
                },
                &mut stream,
            );

            assert!(
                matches!(
                    res,
                    Err(ParseError::Read(ref e)) if e.kind() == io::ErrorKind::Other
                        && e.to_string() == "stts header read err",
                ),
                "SttsAtom::parse_from_stream() returned an invalid value.\nThe parser should \
                    return an error when the 'stts' atom header cannot be read.\n\nExpected: \
                    Err(Read(Custom {{ kind: Other, error: \"stts header read err\" }}))\n\
                    Got:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_fails_on_invalid_version() {
            let mut data = include!("../tests/fixtures/moov.rs")
                .into_iter()
                .skip(679)
                .take(24)
                .collect::<Vec<_>>();

            data[8] = 255;

            let mut stream = TestStream {
                stream: io::Cursor::new(data),
                on_read: |_| Ok(()),
            };

            stream
                .seek(io::SeekFrom::Start(8))
                .expect("Failed to seek the test stream");

            let res = SttsAtom::parse_from_stream(
                AtomBounds {
                    position: 282,
                    size: AtomSize(Size::Standard(24)),
                },
                &mut stream,
            );

            assert!(
                matches!(res, Err(ParseError::AtomVersion(255, AtomType::Stts))),
                "SttsAtom::parse_from_stream() returned an invalid value.\nThe parser should \
                    return an error when the 'stts' atom version field contains an invalid value.\
                    \n\nExpected: Err(AtomVersion(255, Stts))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_fails_if_unable_to_read_sample_count() {
            let mut stream = get_test_stts_stream(|stream| {
                (stream.position() != 29)
                    .then_some(())
                    .ok_or(io::Error::other("stts sample count read err"))
            });

            let res = SttsAtom::parse_from_stream(
                AtomBounds {
                    position: 17,
                    size: AtomSize(Size::Standard(24)),
                },
                &mut stream,
            );

            assert!(
                matches!(
                    res,
                    Err(ParseError::Read(ref e)) if e.kind() == io::ErrorKind::Other
                        && e.to_string() == "stts sample count read err",
                ),
                "SttsAtom::parse_from_stream() returned an invalid value.\nThe parser should \
                    return an error when the 'stts' atom sample count field cannot be read.\n\n\
                    Expected: Err(Read(Custom {{ kind: Other, error: \"stts sample count read \
                    err\" }}))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_fails_if_video_is_fragmented() {
            let mut data = include!("../tests/fixtures/moov.rs")
                .into_iter()
                .skip(679)
                .take(24)
                .collect::<Vec<_>>();

            data[15] = 0;

            let mut stream = TestStream {
                stream: io::Cursor::new(data),
                on_read: |_| Ok(()),
            };

            stream
                .seek(io::SeekFrom::Start(8))
                .expect("Failed to seek the test stream");

            let res = SttsAtom::parse_from_stream(
                AtomBounds {
                    position: 381,
                    size: AtomSize(Size::Standard(24)),
                },
                &mut stream,
            );

            assert!(
                matches!(res, Err(ParseError::Fragmented)),
                "SttsAtom::parse_from_stream() returned an invalid value.\nThe parser should \
                    return an error when the 'stts' atom sample count is 0, as this indicates a \
                    fragmented file.\n\nExpected: Err(Fragmented)\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_fails_if_unable_to_read_the_samples() {
            let mut stream = get_test_stts_stream(|stream| {
                (stream.position() != 33)
                    .then_some(())
                    .ok_or(io::Error::other("stts sample read err"))
            });

            let res = SttsAtom::parse_from_stream(
                AtomBounds {
                    position: 17,
                    size: AtomSize(Size::Standard(24)),
                },
                &mut stream,
            );

            assert!(
                matches!(
                    res,
                    Err(ParseError::Read(ref e)) if e.kind() == io::ErrorKind::Other
                        && e.to_string() == "stts sample read err",
                ),
                "SttsAtom::parse_from_stream() returned an invalid value.\nThe parser should \
                    return an error when the 'stts' atom samples cannot be read.\n\nExpected: \
                    Err(Read(Custom {{ kind: Other, error: \"stts sample read err\" }}))\
                    \nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_returns_valid_stts_atom() {
            let mut stream = get_test_stts_stream(|_| Ok(()));

            let res = SttsAtom::parse_from_stream(
                AtomBounds {
                    position: 17,
                    size: AtomSize(Size::Standard(24)),
                },
                &mut stream,
            );

            assert!(
                matches!(
                    &res,
                    Ok(SttsAtom {
                        _bounds: AtomBounds {
                            position: 17,
                            size: AtomSize(Size::Standard(24)),
                        },
                        table,
                    }) if table == &vec![(30, 512)],
                ),
                "SttsAtom::parse_from_stream() returned an invalid value.\nThe parser should \
                    return a valid 'stts' atom when provided with a valid input stream.\n\n\
                    Expected: Ok(SttsAtom {{ bounds: AtomBounds {{ position: 17, size: \
                    AtomSize(Standard(24)) }}, table: [(30, 512)])\nGot:      {res:?}",
            );

            let stts = res.unwrap();

            let res = stts.frame_count();
            assert!(
                matches!(res, Ok(30)),
                "SttsAtom::frame_count() returned an invalid value.\n\nExpected: Ok(30)\n\
                    Got:      {res:?}",
            );

            let res = stts.total_time_units();
            assert!(
                matches!(res, Ok(15360)),
                "SttsAtom::total_time_units() returned an invalid value.\n\nExpected: Ok(30)\n\
                    Got:      {res:?}",
            );
        }

        #[test]
        fn total_time_units_fails_if_calculation_overflows() {
            let stts = SttsAtom {
                _bounds: AtomBounds {
                    position: 17,
                    size: AtomSize(Size::Standard(24)),
                },
                table: vec![(u32::MAX, u32::MAX); 2],
            };

            let res = stts.total_time_units();

            assert!(
                matches!(res, Err(ParseError::MathError(AtomType::Stts))),
                "SttsAtom::total_time_units() returned an invalid value.\nAn error should be \
                    returned when the calculation overflows the 32-bit limit.\n\nExpected: \
                    Err(MathError(Stts))\nGot:      {res:?}",
            );
        }
    }

    mod mdhd_atom {
        use super::{
            super::ParseError, AtomBounds, AtomHeader, AtomSize, AtomType, MdhdAtom, Size,
            TestStream,
        };
        use std::io;

        fn get_test_mdhd_v0_stream<OnRead>(on_read: OnRead) -> impl io::Seek + io::Read
        where
            OnRead: Fn(&mut io::Cursor<[u8; 24]>) -> io::Result<()>,
        {
            TestStream {
                stream: io::Cursor::new([
                    0, 0, 0, 0, // version + flags
                    0, 0, 0, 0, // creation_time
                    0, 0, 0, 0, // modification_time
                    0, 0, 60, 0, // timescale
                    0, 2, 88, 0, // duration
                    101, 110, // language
                    0, 0, // pre_defined
                ]),
                on_read,
            }
        }

        fn get_test_mdhd_v1_stream<OnRead>(on_read: OnRead) -> impl io::Seek + io::Read
        where
            OnRead: Fn(&mut io::Cursor<[u8; 36]>) -> io::Result<()>,
        {
            TestStream {
                stream: io::Cursor::new([
                    1, 0, 0, 0, // version + flags
                    0, 0, 0, 0, 0, 0, 0, 0, // creation_time
                    0, 0, 0, 0, 0, 0, 0, 0, // modification_time
                    0, 0, 65, 0, // timescale
                    0, 0, 0, 0, 0, 2, 88, 0, // duration
                    101, 110, // language
                    0, 0, // pre_defined
                ]),
                on_read,
            }
        }

        #[test]
        fn parse_from_stream_fails_on_header_read_error() {
            let mut stream = get_test_mdhd_v0_stream(|_| Err(io::Error::other("test error")));

            let res = MdhdAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 130,
                        size: AtomSize(Size::Standard(32)),
                    },
                    atom_type: AtomType::Mdhd,
                },
                &mut stream,
            );

            assert!(
                matches!(
                    res,
                    Err(ParseError::Read(ref e))
                        if e.kind() == io::ErrorKind::Other && e.to_string() == "test error",
                ),
                "MdhdAtom::parse_from_stream() returned an invalid value.\nThe parser should fail \
                    when the 'mdhd' atom flags cannot be read.\n\nExpected: Err(Read(Custom {{ \
                    kind: Other, error: \"test error\" }}))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_fails_on_v0_content_read_error() {
            let mut stream = get_test_mdhd_v0_stream(|cur| {
                (cur.position() != 4)
                    .then_some(())
                    .ok_or(io::Error::other("v0 test failure"))
            });

            let res = MdhdAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 834,
                        size: AtomSize(Size::Standard(32)),
                    },
                    atom_type: AtomType::Mdhd,
                },
                &mut stream,
            );

            assert!(
                matches!(
                    res,
                    Err(ParseError::Read(ref e))
                        if e.kind() == io::ErrorKind::Other && e.to_string() == "v0 test failure",
                ),
                "MdhdAtom::parse_from_stream() returned an invalid value.\nThe parser should fail \
                    when the version 0 'mdhd' atom content cannot be read.\n\nExpected: Err(Read(\
                    Custom {{ kind: Other, error: \"v0 test failure\" }}))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_fails_on_v1_content_read_error() {
            let mut stream = get_test_mdhd_v1_stream(|cur| {
                (cur.position() != 4)
                    .then_some(())
                    .ok_or(io::Error::other("v1 test failure"))
            });

            let res = MdhdAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 834,
                        size: AtomSize(Size::Standard(44)),
                    },
                    atom_type: AtomType::Mdhd,
                },
                &mut stream,
            );

            assert!(
                matches!(
                    res,
                    Err(ParseError::Read(ref e))
                        if e.kind() == io::ErrorKind::Other && e.to_string() == "v1 test failure",
                ),
                "MdhdAtom::parse_from_stream() returned an invalid value.\nThe parser should fail \
                    when the version 1 'mdhd' atom content cannot be read.\n\nExpected: Err(Read(\
                    Custom {{ kind: Other, error: \"v1 test failure\" }}))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_stream_invalid_version() {
            let mut stream = TestStream {
                stream: io::Cursor::new([3, 0, 0, 0]),
                on_read: |_| Ok(()),
            };

            let res = MdhdAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 732,
                        size: AtomSize(Size::Standard(32)),
                    },
                    atom_type: AtomType::Mdhd,
                },
                &mut stream,
            );

            assert!(
                matches!(res, Err(ParseError::AtomVersion(3, AtomType::Mdhd))),
                "MdhdAtom::parse_from_stream() returned an invalid value.\nThe parser should fail \
                    when the 'mdhd' atom contains an invalid version field.\n\nExpected: Err(\
                    AtomVersion(3, Mdhd))\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_valid_stream_returns_mdhd_v0_atom() {
            let mut stream = get_test_mdhd_v0_stream(|_| Ok(()));

            let res = MdhdAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 64,
                        size: AtomSize(Size::Standard(32)),
                    },
                    atom_type: AtomType::Mdhd,
                },
                &mut stream,
            );

            assert!(
                matches!(
                    res,
                    Ok(MdhdAtom {
                        _bounds: AtomBounds {
                            position: 64,
                            size: AtomSize(Size::Standard(32)),
                        },
                        timescale: 15360,
                    }),
                ),
                "MdhdAtom::parse_from_stream() returned an invalid value.\nParsing failed to \
                    return a valid version 0 'mdhd' atom from the provided stream.\n\nExpected: \
                    Ok(MdhdAtom {{ bounds: AtomBounds {{ position: 64, size: \
                    AtomSize(Standard(32)) }}, timescale: 15360 }})\nGot:      {res:?}",
            );
        }

        #[test]
        fn parse_from_valid_stream_returns_mdhd_v1_atom() {
            let mut stream = get_test_mdhd_v1_stream(|_| Ok(()));

            let res = MdhdAtom::parse_from_stream(
                AtomHeader {
                    bounds: AtomBounds {
                        position: 8311,
                        size: AtomSize(Size::Standard(44)),
                    },
                    atom_type: AtomType::Mdhd,
                },
                &mut stream,
            );

            assert!(
                matches!(
                    res,
                    Ok(MdhdAtom {
                        _bounds: AtomBounds {
                            position: 8311,
                            size: AtomSize(Size::Standard(44)),
                        },
                        timescale: 16640,
                    }),
                ),
                "MdhdAtom::parse_from_stream() returned an invalid value.\nParsing failed to \
                    return a valid version 0 'mdhd' atom from the provided stream.\n\nExpected: \
                    Ok(MdhdAtom {{ bounds: AtomBounds {{ position: 8311, size: \
                    AtomSize(Standard(44)) }}, timescale: 16640 }})\nGot:      {res:?}",
            );
        }
    }
}
