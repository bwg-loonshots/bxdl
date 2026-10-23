//! Small strict USTAR codec for the fixed CLI-only inventory. No GNU/PAX,
//! links, directories, sparse files or concatenated archives are accepted.
use super::*;

pub(super) fn read(raw: &[u8]) -> Result<BTreeMap<String, (u32, Vec<u8>)>> {
    let mut offset = 0;
    let mut entries = BTreeMap::new();
    loop {
        let h = raw.get(offset..offset + 512).ok_or_else(invalid)?;
        offset += 512;
        if zero(h) {
            if !zero(raw.get(offset..offset + 512).ok_or_else(invalid)?) {
                return Err(invalid());
            }
            offset += 512;
            if raw.len() - offset > 1 << 20 || !zero(&raw[offset..]) {
                return Err(invalid());
            }
            break;
        }
        if entries.len() >= 6
            || ![0, b'0'].contains(&h[156])
            || &h[257..263] != b"ustar\0"
            || &h[263..265] != b"00"
            || !zero(&h[157..257])
            || !zero(&h[500..])
        {
            return Err(invalid());
        }
        let checksum: u64 = h
            .iter()
            .enumerate()
            .map(|(i, b)| {
                if (148..156).contains(&i) {
                    u64::from(b' ')
                } else {
                    u64::from(*b)
                }
            })
            .sum();
        if number(&h[148..156])? != checksum {
            return Err(invalid());
        }
        let name = string(&h[..100])?;
        if !string(&h[345..500])?.is_empty() {
            return Err(invalid());
        }
        if !matches!(name, "manifest.json" | "manifest.sig")
            && !PAYLOADS.iter().any(|(p, _)| *p == name)
        {
            return Err(invalid());
        }
        if entries.is_empty() && name != "manifest.json" {
            return Err(invalid());
        }
        let mode = number(&h[100..108])?;
        let size = number(&h[124..136])?;
        if size == 0 || size > limit(name) as u64 || ![0o644, 0o755].contains(&mode) {
            return Err(invalid());
        }
        for field in [
            &h[108..116],
            &h[116..124],
            &h[136..148],
            &h[329..337],
            &h[337..345],
        ] {
            number(field)?;
        }
        let size = size as usize;
        let bytes = raw.get(offset..offset + size).ok_or_else(invalid)?.to_vec();
        offset += size;
        let padding = (512 - size % 512) % 512;
        if !zero(raw.get(offset..offset + padding).ok_or_else(invalid)?) {
            return Err(invalid());
        }
        offset += padding;
        if entries
            .insert(name.to_owned(), (mode as u32, bytes))
            .is_some()
        {
            return Err(invalid());
        }
    }
    Ok(entries)
}
fn zero(bytes: &[u8]) -> bool {
    bytes.iter().all(|b| *b == 0)
}
fn string(bytes: &[u8]) -> Result<&str> {
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    if !zero(&bytes[end..]) {
        return Err(invalid());
    }
    std::str::from_utf8(&bytes[..end]).map_err(|_| invalid())
}
fn number(bytes: &[u8]) -> Result<u64> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| invalid())?
        .trim_matches(['\0', ' ']);
    if text.is_empty() {
        return Ok(0);
    }
    if !text.bytes().all(|b| (b'0'..=b'7').contains(&b)) {
        return Err(invalid());
    }
    u64::from_str_radix(text, 8).map_err(|_| invalid())
}
pub(super) fn write(entries: &BTreeMap<String, (u32, Vec<u8>)>) -> Result<Vec<u8>> {
    let mut raw = Vec::new();
    let order = ["manifest.json", "manifest.sig"]
        .into_iter()
        .chain(PAYLOADS.iter().map(|(p, _)| *p));
    for name in order {
        let Some((mode, bytes)) = entries.get(name) else {
            continue;
        };
        let mut h = [0; 512];
        h[..name.len()].copy_from_slice(name.as_bytes());
        octal(&mut h[100..108], u64::from(*mode))?;
        octal(&mut h[108..116], 0)?;
        octal(&mut h[116..124], 0)?;
        octal(&mut h[124..136], bytes.len() as u64)?;
        octal(&mut h[136..148], 0)?;
        h[148..156].fill(b' ');
        h[156] = b'0';
        h[257..263].copy_from_slice(b"ustar\0");
        h[263..265].copy_from_slice(b"00");
        let checksum: u64 = h.iter().map(|b| u64::from(*b)).sum();
        h[148..154].copy_from_slice(format!("{checksum:06o}").as_bytes());
        h[154] = 0;
        h[155] = b' ';
        raw.extend(h);
        raw.extend(bytes);
        raw.resize(raw.len() + (512 - bytes.len() % 512) % 512, 0);
    }
    raw.extend([0; 1024]);
    if raw.len() > MAX_RAW {
        return Err(invalid());
    }
    Ok(raw)
}
fn octal(field: &mut [u8], value: u64) -> Result<()> {
    let text = format!("{value:o}");
    if text.len() >= field.len() {
        return Err(invalid());
    }
    field.fill(b'0');
    let end = field.len() - 1;
    field[end - text.len()..end].copy_from_slice(text.as_bytes());
    field[end] = 0;
    Ok(())
}
