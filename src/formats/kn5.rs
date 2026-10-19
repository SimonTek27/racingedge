//! `.kn5` — mesh container (port of ksengine `Kn5Reader.*`).
//!
//! Formato binario little-endian, verificato byte-per-byte su ~2770 file reali.
//! The magic is the ASCII string `sc6969` (not `KN5\0`/`0x346E6B73` as assumed
//! by the old `KN5Parser`).
//!
//! ```text
//! char   magic[6]     = "sc6969"
//! int32  version      = 4 | 5 | 6
//! int32  extra        (solo se version > 5)
//! int32  textureCount, per texture:
//!          int32 active; active == 0 -> record finisce qui
//!          int32 nameLen + name + uint32 dataLen + data (.dds)
//! int32  materialCount, per material:
//!          string name, string shader, u8 blend, u8 alphaTested,
//!          int32 depthMode (solo version >= 5)
//!          int32 propertyCount { string name + 40 byte }
//!          int32 mappingCount  { string name + int32 slot + string texture }
//! node hierarchy: root, poi figli ricorsivamente (preorder):
//!          int32 nodeClass (1 Base | 2 Mesh | 3 SkinnedMesh),
//!          string name, int32 childrenCount, u8 active, payload
//! ```
//!
//! Le matrici usano la convenzione DirectX a vettore-riga (`p' = p * M`), quindi
//! la traslazione vive in `[12..14]` e un figlio compone `world = local * parent`.
//!
//! Il port mantiene la fedeltà del reader C++: limiti su count/profondità,
//! tolerance to trailing bytes after the node tree and to non-ASCII names, and
//! rejection of two known encrypted files (magic check).

use std::path::Path;

/// Magic di un file KN5 (`sc6969`).
pub const KN5_MAGIC: [u8; 6] = *b"sc6969";
/// Versione minima supportata.
pub const MIN_VERSION: i32 = 4;
/// Versione massima supportata.
pub const MAX_VERSION: i32 = 6;
/// Numero di float in una matrice 4x4.
pub const MATRIX_FLOATS: usize = 16;

/// Matrice identità row-major (traslazione in `[12..14]`).
pub const IDENTITY_MATRIX: [f32; MATRIX_FLOATS] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

// Limits mirroring the reference Python validated on the whole `.kn5` corpus.
const MAX_RECORDS: i32 = 100_000;
const MAX_PROPERTIES: i32 = 10_000;
const MAX_MAPPINGS: i32 = 10_000;
const MAX_BONES: i32 = 1_000;
const MAX_DEPTH: i32 = 64;

const MESH_VERTEX_BYTES: usize = 44; // pos3+normal3+uv2+tangent3
const SKINNED_VERTEX_BYTES: usize = 76;
const MESH_TAIL_BYTES: usize = 33; // materialId..isRenderable
const SKINNED_TAIL_BYTES: usize = 16; // materialId..lodOut
const MATRIX_BYTES: usize = 16 * 4;

/// Classe di un nodo KN5.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Kn5NodeClass {
    #[default]
    Base = 1,
    Mesh = 2,
    SkinnedMesh = 3,
}

/// Una texture (payload `.dds` conservato solo se richiesto).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Kn5Texture {
    pub name: String,
    pub data_size: u32,
    pub data: Vec<u8>,
}

/// Una proprietà di shader: nome + blob opaco di 40 byte.
#[derive(Clone, Debug, PartialEq)]
pub struct Kn5Property {
    pub name: String,
    pub value: [u8; 40],
}

impl Default for Kn5Property {
    fn default() -> Self {
        Self {
            name: String::new(),
            value: [0; 40],
        }
    }
}

/// Un mapping nome→slot→texture di un materiale.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Kn5TextureMapping {
    pub name: String,
    pub slot: i32,
    pub texture: String,
}

/// Un materiale.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Kn5Material {
    pub name: String,
    pub shader: String,
    pub blend: u8,
    pub alpha_tested: u8,
    /// 0 per i file versione 4 (campo assente).
    pub depth_mode: i32,
    pub properties: Vec<Kn5Property>,
    pub mappings: Vec<Kn5TextureMapping>,
}

/// Una mesh (posizioni/normali/uv/indici, mesh-local).
#[derive(Clone, Debug, PartialEq)]
pub struct Kn5Mesh {
    pub name: String,
    pub material_id: i32,
    pub layer: u32,
    /// Finestra di distanza autore: visibile in `[lod_in, lod_out]` metri.
    pub lod_in: f32,
    pub lod_out: f32,
    /// Trasformazione assoluta, accumulata dai Base antenati al parsing.
    pub world: [f32; MATRIX_FLOATS],
    pub positions: Vec<f32>,
    pub normals: Vec<f32>,
    pub uvs: Vec<f32>,
    pub indices: Vec<u32>,
}

impl Default for Kn5Mesh {
    fn default() -> Self {
        Self {
            name: String::new(),
            material_id: -1,
            layer: 0,
            lod_in: 0.0,
            lod_out: 1.0e9,
            world: IDENTITY_MATRIX,
            positions: Vec::new(),
            normals: Vec::new(),
            uvs: Vec::new(),
            indices: Vec::new(),
        }
    }
}

impl Kn5Mesh {
    /// Numero di vertici.
    pub fn vertex_count(&self) -> usize {
        self.positions.len() / 3
    }

    /// Numero di triangoli (indici/3).
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
}

/// Un nodo dell'albero (Base, Mesh o SkinnedMesh).
#[derive(Clone, Debug, PartialEq)]
pub struct Kn5Node {
    pub node_class: Kn5NodeClass,
    pub name: String,
    pub active: u8,
    /// Trasformazione locale (solo Base).
    pub transform: [f32; MATRIX_FLOATS],
    pub has_mesh: bool,
    pub mesh: Kn5Mesh,
    pub children: Vec<Kn5Node>,
}

impl Default for Kn5Node {
    fn default() -> Self {
        Self {
            node_class: Kn5NodeClass::Base,
            name: String::new(),
            active: 1,
            transform: IDENTITY_MATRIX,
            has_mesh: false,
            mesh: Kn5Mesh::default(),
            children: Vec::new(),
        }
    }
}

/// Un file KN5 completo.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Kn5File {
    pub version: i32,
    pub textures: Vec<Kn5Texture>,
    pub materials: Vec<Kn5Material>,
    /// Esattamente un root, secondo il formato.
    pub nodes: Vec<Kn5Node>,
}

impl Kn5File {
    /// Visita in preordine tutti i nodi (root inclusi i discendenti).
    pub fn walk<'a>(&'a self, mut f: impl FnMut(&'a Kn5Node)) {
        fn rec<'a>(node: &'a Kn5Node, f: &mut impl FnMut(&'a Kn5Node)) {
            f(node);
            for c in &node.children {
                rec(c, f);
            }
        }
        for n in &self.nodes {
            rec(n, &mut f);
        }
    }

    /// Tutte le mesh presenti nell'albero.
    pub fn meshes(&self) -> Vec<&Kn5Mesh> {
        let mut out = Vec::new();
        self.walk(|n| {
            if n.has_mesh {
                out.push(&n.mesh);
            }
        });
        out
    }
}

/// Risultato del parsing.
#[derive(Clone, Debug)]
pub struct Kn5ParseResult {
    pub file: Kn5File,
    /// Errore `offset N: ...`, vuoto in caso di successo.
    pub error: String,
}

impl Kn5ParseResult {
    pub fn ok(&self) -> bool {
        self.error.is_empty()
    }
}

/// Opzioni di parsing.
#[derive(Clone, Copy, Debug, Default)]
pub struct Kn5ParseOptions {
    /// Conserva i byte `.dds` di ogni texture (per il baker; la runtime no).
    pub keep_texture_data: bool,
}

/// `true` se i primi 6 byte sono il magic KN5.
pub fn is_kn5(bytes: &[u8]) -> bool {
    bytes.len() >= 6 && bytes[..6] == KN5_MAGIC
}

/// Parsing di un KN5 in memoria (binary-safe).
pub fn parse_kn5(bytes: &[u8], options: Kn5ParseOptions) -> Kn5ParseResult {
    let mut result = Kn5ParseResult {
        file: Kn5File::default(),
        error: String::new(),
    };
    let mut r = ByteReader::new(bytes);

    let magic = match r.read(6) {
        Some(m) => m,
        None => {
            result.error = r.take_error();
            return result;
        }
    };
    if magic != KN5_MAGIC {
        result.error = "offset 0: bad KN5 magic (expected \"sc6969\")".into();
        return result;
    }

    result.file.version = match r.read_i32() {
        Some(v) => v,
        None => {
            result.error = r.take_error();
            return result;
        }
    };
    if !(MIN_VERSION..=MAX_VERSION).contains(&result.file.version) {
        r.fail(format!("unsupported KN5 version {}", result.file.version));
        result.error = r.take_error();
        return result;
    }
    if result.file.version > 5 && !r.skip(4) {
        result.error = r.take_error();
        return result;
    }

    if !parse_textures(&mut r, &mut result.file.textures, options.keep_texture_data)
        || !parse_materials(&mut r, result.file.version, &mut result.file.materials)
    {
        result.error = r.take_error();
        return result;
    }

    let mut root = Kn5Node::default();
    if !parse_node(&mut r, IDENTITY_MATRIX, 0, &mut root) {
        result.error = r.take_error();
        return result;
    }
    result.file.nodes.push(root);
    result
}

/// Legge e parsia un file `.kn5` da disco.
pub fn parse_kn5_file(path: impl AsRef<Path>, options: Kn5ParseOptions) -> Kn5ParseResult {
    let path = path.as_ref();
    match std::fs::read(path) {
        Ok(bytes) => parse_kn5(&bytes, options),
        Err(e) => Kn5ParseResult {
            file: Kn5File::default(),
            error: format!("cannot open file {}: {e}", path.display()),
        },
    }
}

// ── Parsing ──────────────────────────────────────────────────────────────────

fn parse_textures(r: &mut ByteReader, out: &mut Vec<Kn5Texture>, keep_data: bool) -> bool {
    let count = match r.read_count(MAX_RECORDS, "texture") {
        Some(c) => c,
        None => return false,
    };
    for _ in 0..count {
        let active = match r.read_i32() {
            Some(a) => a,
            None => return false,
        };
        if active == 0 {
            continue;
        }
        let name = match r.read_string() {
            Some(s) => s,
            None => return false,
        };
        let data_len = match r.read_u32() {
            Some(d) => d,
            None => return false,
        };
        let mut data = Vec::new();
        if keep_data {
            if !r.read_into(&mut data, data_len as usize) {
                return false;
            }
        } else if !r.skip(data_len as usize) {
            return false;
        }
        out.push(Kn5Texture {
            name,
            data_size: data_len,
            data,
        });
    }
    true
}

fn parse_materials(r: &mut ByteReader, version: i32, out: &mut Vec<Kn5Material>) -> bool {
    let count = match r.read_count(MAX_RECORDS, "material") {
        Some(c) => c,
        None => return false,
    };
    for _ in 0..count {
        let mut m = Kn5Material {
            name: match r.read_string() {
                Some(s) => s,
                None => return false,
            },
            shader: match r.read_string() {
                Some(s) => s,
                None => return false,
            },
            blend: match r.read_u8() {
                Some(b) => b,
                None => return false,
            },
            alpha_tested: match r.read_u8() {
                Some(b) => b,
                None => return false,
            },
            ..Default::default()
        };
        if version >= 5 {
            m.depth_mode = match r.read_i32() {
                Some(d) => d,
                None => return false,
            };
        }

        let properties = match r.read_count(MAX_PROPERTIES, "material property") {
            Some(c) => c,
            None => return false,
        };
        for _ in 0..properties {
            let name = match r.read_string() {
                Some(s) => s,
                None => return false,
            };
            let mut value = [0u8; 40];
            match r.read(40) {
                Some(b) => value.copy_from_slice(b),
                None => return false,
            }
            m.properties.push(Kn5Property { name, value });
        }

        let mappings = match r.read_count(MAX_MAPPINGS, "texture mapping") {
            Some(c) => c,
            None => return false,
        };
        for _ in 0..mappings {
            let name = match r.read_string() {
                Some(s) => s,
                None => return false,
            };
            let slot = match r.read_i32() {
                Some(s) => s,
                None => return false,
            };
            let texture = match r.read_string() {
                Some(s) => s,
                None => return false,
            };
            m.mappings.push(Kn5TextureMapping {
                name,
                slot,
                texture,
            });
        }
        out.push(m);
    }
    true
}

fn parse_mesh_payload(r: &mut ByteReader, mesh: &mut Kn5Mesh) -> bool {
    if !r.skip(3) {
        return false; // castShadows/visible/transparent
    }

    let vertex_count = match r.read_u32() {
        Some(v) => v,
        None => return false,
    };
    if !r.bytes_available(vertex_count as u64, MESH_VERTEX_BYTES as u64, "vertex data") {
        return false;
    }
    mesh.positions.reserve(vertex_count as usize * 3);
    mesh.normals.reserve(vertex_count as usize * 3);
    mesh.uvs.reserve(vertex_count as usize * 2);
    for _ in 0..vertex_count {
        let v = match r.read(MESH_VERTEX_BYTES) {
            Some(b) => b,
            None => return false,
        };
        let f = |i: usize| f32::from_le_bytes(v[i * 4..i * 4 + 4].try_into().unwrap());
        mesh.positions.extend_from_slice(&[f(0), f(1), f(2)]);
        mesh.normals.extend_from_slice(&[f(3), f(4), f(5)]);
        mesh.uvs.extend_from_slice(&[f(6), f(7)]);
    }

    let index_count = match r.read_u32() {
        Some(v) => v,
        None => return false,
    };
    if !r.bytes_available(index_count as u64, 2, "index data") {
        return false;
    }
    mesh.indices.reserve(index_count as usize);
    for _ in 0..index_count {
        let index = match r.read_u16() {
            Some(i) => i,
            None => return false,
        };
        if index as u32 >= vertex_count {
            return r.fail(format!(
                "index {index} out of range (vertex count {vertex_count})"
            ));
        }
        mesh.indices.push(index as u32);
    }

    mesh.material_id = match r.read_i32() {
        Some(v) => v,
        None => return false,
    };
    mesh.layer = match r.read_u32() {
        Some(v) => v,
        None => return false,
    };
    mesh.lod_in = match r.read_f32() {
        Some(v) => v,
        None => return false,
    };
    mesh.lod_out = match r.read_f32() {
        Some(v) => v,
        None => return false,
    };
    r.skip(MESH_TAIL_BYTES - 16)
}

fn parse_skinned_payload(r: &mut ByteReader) -> bool {
    if !r.skip(3) {
        return false;
    }
    let bones = match r.read_count(MAX_BONES, "bone") {
        Some(b) => b,
        None => return false,
    };
    for _ in 0..bones {
        if r.read_string().is_none() || !r.skip(MATRIX_BYTES) {
            return false;
        }
    }

    let vertex_count = match r.read_u32() {
        Some(v) => v,
        None => return false,
    };
    if !r.bytes_available(
        vertex_count as u64,
        SKINNED_VERTEX_BYTES as u64,
        "skinned vertex data",
    ) {
        return false;
    }
    if !r.skip(vertex_count as usize * SKINNED_VERTEX_BYTES) {
        return false;
    }

    let index_count = match r.read_u32() {
        Some(v) => v,
        None => return false,
    };
    if !r.bytes_available(index_count as u64, 2, "skinned index data") {
        return false;
    }
    r.skip(index_count as usize * 2 + SKINNED_TAIL_BYTES)
}

fn parse_node(
    r: &mut ByteReader,
    parent_world: [f32; MATRIX_FLOATS],
    depth: i32,
    out: &mut Kn5Node,
) -> bool {
    if depth > MAX_DEPTH {
        return r.fail("node nesting too deep");
    }

    let class_value = match r.read_i32() {
        Some(v) => v,
        None => return false,
    };
    out.node_class = match class_value {
        1 => Kn5NodeClass::Base,
        2 => Kn5NodeClass::Mesh,
        3 => Kn5NodeClass::SkinnedMesh,
        other => return r.fail(format!("unknown node class {other}")),
    };

    out.name = match r.read_string() {
        Some(s) => s,
        None => return false,
    };
    let children = match r.read_count(MAX_RECORDS, "node child") {
        Some(c) => c,
        None => return false,
    };
    out.active = match r.read_u8() {
        Some(a) => a,
        None => return false,
    };

    let mut world = parent_world;
    match out.node_class {
        Kn5NodeClass::Base => {
            let bytes = match r.read(MATRIX_BYTES) {
                Some(b) => b,
                None => return false,
            };
            for (i, slot) in out.transform.iter_mut().enumerate() {
                let o = i * 4;
                *slot = f32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
            }
            world = multiply_matrices(out.transform, parent_world);
        }
        Kn5NodeClass::Mesh => {
            if !parse_mesh_payload(r, &mut out.mesh) {
                return false;
            }
            out.has_mesh = true;
            out.mesh.name = out.name.clone();
            out.mesh.world = world;
        }
        Kn5NodeClass::SkinnedMesh => {
            if !parse_skinned_payload(r) {
                return false;
            }
        }
    }

    for _ in 0..children {
        let mut child = Kn5Node::default();
        if !parse_node(r, world, depth + 1, &mut child) {
            return false;
        }
        out.children.push(child);
    }
    true
}

/// Prodotto row-major per la convenzione a vettore-riga: `out = a * b`.
pub fn multiply_matrices(a: [f32; MATRIX_FLOATS], b: [f32; MATRIX_FLOATS]) -> [f32; MATRIX_FLOATS] {
    let mut out = [0.0f32; MATRIX_FLOATS];
    for r in 0..4 {
        for c in 0..4 {
            let mut sum = 0.0;
            for k in 0..4 {
                sum += a[r * 4 + k] * b[k * 4 + c];
            }
            out[r * 4 + c] = sum;
        }
    }
    out
}

// ── Cursor binario little-endian ─────────────────────────────────────────────

struct ByteReader<'a> {
    data: &'a [u8],
    pos: usize,
    error: String,
}

impl<'a> ByteReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            error: String::new(),
        }
    }

    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    fn fail(&mut self, msg: impl Into<String>) -> bool {
        if self.error.is_empty() {
            self.error = format!("offset {}: {}", self.pos, msg.into());
        }
        false
    }

    fn take_error(&mut self) -> String {
        std::mem::take(&mut self.error)
    }

    fn read(&mut self, n: usize) -> Option<&'a [u8]> {
        if n > self.remaining() {
            self.fail("unexpected end of file");
            return None;
        }
        let slice = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Some(slice)
    }

    fn skip(&mut self, n: usize) -> bool {
        if n > self.remaining() {
            return self.fail("payload extends past end of file");
        }
        self.pos += n;
        true
    }

    fn read_into(&mut self, out: &mut Vec<u8>, n: usize) -> bool {
        match self.read(n) {
            Some(b) => {
                out.extend_from_slice(b);
                true
            }
            None => false,
        }
    }

    fn read_u8(&mut self) -> Option<u8> {
        self.read(1).map(|b| b[0])
    }

    fn read_u16(&mut self) -> Option<u16> {
        self.read(2)
            .map(|b| u16::from_le_bytes(b.try_into().unwrap()))
    }

    fn read_u32(&mut self) -> Option<u32> {
        self.read(4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
    }

    fn read_i32(&mut self) -> Option<i32> {
        self.read(4)
            .map(|b| i32::from_le_bytes(b.try_into().unwrap()))
    }

    fn read_f32(&mut self) -> Option<f32> {
        self.read(4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
    }

    fn read_string(&mut self) -> Option<String> {
        let len = self.read_i32()?;
        if len < 0 {
            self.fail("negative string length");
            return None;
        }
        let bytes = self.read(len as usize)?;
        Some(String::from_utf8_lossy(bytes).into_owned())
    }

    fn read_count(&mut self, limit: i32, what: &str) -> Option<i32> {
        let count = self.read_i32()?;
        if count < 0 || count > limit {
            self.fail(format!("implausible {what} count {count}"));
            return None;
        }
        Some(count)
    }

    fn bytes_available(&mut self, n: u64, stride: u64, what: &str) -> bool {
        if stride != 0 && n > self.remaining() as u64 / stride {
            return self.fail(format!("{what} extends past end of file"));
        }
        true
    }
}

// ── Conversione Bevy ─────────────────────────────────────────────────────────

/// Converte una mesh KN5 in una mesh Bevy (posizioni/normali/uv/indici).
pub fn to_bevy_mesh(mesh: &Kn5Mesh) -> bevy::render::mesh::Mesh {
    use bevy::asset::RenderAssetUsages;
    use bevy::render::mesh::{Indices, Mesh, PrimitiveTopology};

    let positions: Vec<[f32; 3]> = mesh
        .positions
        .as_chunks::<3>()
        .0
        .iter()
        .map(|c| [c[0], c[1], c[2]])
        .collect();
    let normals: Vec<[f32; 3]> = mesh
        .normals
        .as_chunks::<3>()
        .0
        .iter()
        .map(|c| [c[0], c[1], c[2]])
        .collect();
    let uvs: Vec<[f32; 2]> = mesh
        .uvs
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| [c[0], c[1]])
        .collect();

    let mut out = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    out.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    if !normals.is_empty() {
        out.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    }
    if !uvs.is_empty() {
        out.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    }
    out.insert_indices(Indices::U32(mesh.indices.clone()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_str(buf: &mut Vec<u8>, s: &str) {
        buf.extend_from_slice(&(s.len() as i32).to_le_bytes());
        buf.extend_from_slice(s.as_bytes());
    }

    fn push_identity(buf: &mut Vec<u8>) {
        for f in IDENTITY_MATRIX {
            buf.extend_from_slice(&f.to_le_bytes());
        }
    }

    fn base_node(buf: &mut Vec<u8>, name: &str, translation: [f32; 3]) {
        buf.extend_from_slice(&1i32.to_le_bytes()); // Base
        push_str(buf, name);
        buf.extend_from_slice(&0i32.to_le_bytes()); // children
        buf.push(1); // active
        let mut m = IDENTITY_MATRIX;
        m[12] = translation[0];
        m[13] = translation[1];
        m[14] = translation[2];
        for f in m {
            buf.extend_from_slice(&f.to_le_bytes());
        }
    }

    fn header(version: i32) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&KN5_MAGIC);
        buf.extend_from_slice(&version.to_le_bytes());
        if version > 5 {
            buf.extend_from_slice(&0i32.to_le_bytes()); // extra
        }
        buf
    }

    fn minimal_v4() -> Vec<u8> {
        let mut buf = header(4);
        buf.extend_from_slice(&0i32.to_le_bytes()); // textures
        buf.extend_from_slice(&0i32.to_le_bytes()); // materials
        base_node(&mut buf, "root", [0.0, 0.0, 0.0]);
        buf
    }

    #[test]
    fn bad_magic_is_rejected() {
        let bytes = b"nope!!\0\0\0\0".to_vec();
        let r = parse_kn5(&bytes, Kn5ParseOptions::default());
        assert!(!r.ok());
        assert!(r.error.contains("magic"), "{}", r.error);
    }

    #[test]
    fn unsupported_version_is_rejected() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&KN5_MAGIC);
        bytes.extend_from_slice(&7i32.to_le_bytes());
        let r = parse_kn5(&bytes, Kn5ParseOptions::default());
        assert!(!r.ok());
        assert!(r.error.contains("unsupported KN5 version"), "{}", r.error);
    }

    #[test]
    fn truncated_file_reports_end_of_file() {
        let r = parse_kn5(&KN5_MAGIC[..3], Kn5ParseOptions::default());
        assert!(!r.ok());
        assert!(r.error.contains("end of file"), "{}", r.error);
    }

    #[test]
    fn parses_minimal_v4_root() {
        let r = parse_kn5(&minimal_v4(), Kn5ParseOptions::default());
        assert!(r.ok(), "{}", r.error);
        assert_eq!(r.file.version, 4);
        assert_eq!(r.file.nodes.len(), 1);
        assert_eq!(r.file.nodes[0].name, "root");
        assert_eq!(r.file.nodes[0].node_class, Kn5NodeClass::Base);
        assert_eq!(r.file.nodes[0].transform, IDENTITY_MATRIX);
    }

    #[test]
    fn v5_material_reads_depth_mode() {
        let mut buf = header(5);
        buf.extend_from_slice(&0i32.to_le_bytes()); // textures
        buf.extend_from_slice(&1i32.to_le_bytes()); // materials
        push_str(&mut buf, "mat");
        push_str(&mut buf, "ksPerPixel");
        buf.push(0); // blend
        buf.push(1); // alpha_tested
        buf.extend_from_slice(&2i32.to_le_bytes()); // depth_mode (v5 only)
        buf.extend_from_slice(&1i32.to_le_bytes()); // properties
        push_str(&mut buf, "diffuseMult");
        buf.extend_from_slice(&[7u8; 40]);
        buf.extend_from_slice(&1i32.to_le_bytes()); // mappings
        push_str(&mut buf, "txDiffuse");
        buf.extend_from_slice(&0i32.to_le_bytes()); // slot
        push_str(&mut buf, "body.dds");
        base_node(&mut buf, "root", [0.0, 0.0, 0.0]);

        let r = parse_kn5(&buf, Kn5ParseOptions::default());
        assert!(r.ok(), "{}", r.error);
        assert_eq!(r.file.materials.len(), 1);
        let m = &r.file.materials[0];
        assert_eq!(m.name, "mat");
        assert_eq!(m.shader, "ksPerPixel");
        assert_eq!(m.depth_mode, 2);
        assert_eq!(m.properties.len(), 1);
        assert_eq!(m.properties[0].name, "diffuseMult");
        assert_eq!(m.properties[0].value, [7u8; 40]);
        assert_eq!(m.mappings[0].texture, "body.dds");
    }

    #[test]
    fn v4_material_has_no_depth_mode() {
        let mut buf = header(4);
        buf.extend_from_slice(&0i32.to_le_bytes()); // textures
        buf.extend_from_slice(&1i32.to_le_bytes()); // materials
        push_str(&mut buf, "mat");
        push_str(&mut buf, "ksPerPixel");
        buf.push(0);
        buf.push(0);
        // no depth_mode in v4
        buf.extend_from_slice(&0i32.to_le_bytes()); // properties
        buf.extend_from_slice(&0i32.to_le_bytes()); // mappings
        base_node(&mut buf, "root", [0.0, 0.0, 0.0]);

        let r = parse_kn5(&buf, Kn5ParseOptions::default());
        assert!(r.ok(), "{}", r.error);
        assert_eq!(r.file.materials[0].depth_mode, 0);
    }

    fn mesh_node(buf: &mut Vec<u8>, name: &str, positions: &[[f32; 3]], indices: &[u16]) {
        buf.extend_from_slice(&2i32.to_le_bytes()); // Mesh
        push_str(buf, name);
        buf.extend_from_slice(&0i32.to_le_bytes()); // children
        buf.push(1); // active
        buf.extend_from_slice(&[0u8; 3]); // flags
        buf.extend_from_slice(&(positions.len() as u32).to_le_bytes());
        for p in positions {
            for f in [p[0], p[1], p[2], 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0] {
                buf.extend_from_slice(&f.to_le_bytes());
            }
        }
        buf.extend_from_slice(&(indices.len() as u32).to_le_bytes());
        for i in indices {
            buf.extend_from_slice(&i.to_le_bytes());
        }
        buf.extend_from_slice(&0i32.to_le_bytes()); // material id
        buf.extend_from_slice(&0u32.to_le_bytes()); // layer
        buf.extend_from_slice(&0.0f32.to_le_bytes()); // lod in
        buf.extend_from_slice(&500.0f32.to_le_bytes()); // lod out
        buf.extend_from_slice(&[0u8; MESH_TAIL_BYTES - 16]); // bsCenter..isRenderable
    }

    #[test]
    fn mesh_node_parses_geometry_and_world_transform() {
        let mut buf = header(4);
        buf.extend_from_slice(&0i32.to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes());
        // root base with translation, then a mesh child.
        buf.extend_from_slice(&1i32.to_le_bytes()); // Base
        push_str(&mut buf, "root");
        buf.extend_from_slice(&1i32.to_le_bytes()); // 1 child
        buf.push(1);
        let mut m = IDENTITY_MATRIX;
        m[12] = 5.0;
        for f in m {
            buf.extend_from_slice(&f.to_le_bytes());
        }
        mesh_node(
            &mut buf,
            "body",
            &[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            &[0, 1, 2],
        );

        let r = parse_kn5(&buf, Kn5ParseOptions::default());
        assert!(r.ok(), "{}", r.error);
        let meshes = r.file.meshes();
        assert_eq!(meshes.len(), 1);
        let mesh = meshes[0];
        assert_eq!(mesh.name, "body");
        assert_eq!(mesh.vertex_count(), 3);
        assert_eq!(mesh.triangle_count(), 1);
        assert_eq!(mesh.lod_out, 500.0);
        // world inherited the parent translation (elements 12..14).
        assert_eq!(mesh.world[12], 5.0);
        assert_eq!(
            mesh.positions,
            vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]
        );
    }

    #[test]
    fn out_of_range_index_is_rejected() {
        let mut buf = header(4);
        buf.extend_from_slice(&0i32.to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes());
        // root with one mesh child whose index points past the vertex count.
        buf.extend_from_slice(&1i32.to_le_bytes());
        push_str(&mut buf, "root");
        buf.extend_from_slice(&1i32.to_le_bytes());
        buf.push(1);
        push_identity(&mut buf);
        mesh_node(&mut buf, "bad", &[[0.0, 0.0, 0.0]], &[0, 3, 0]);

        let r = parse_kn5(&buf, Kn5ParseOptions::default());
        assert!(!r.ok());
        assert!(r.error.contains("out of range"), "{}", r.error);
    }

    #[test]
    fn implausible_count_is_rejected() {
        let mut buf = header(4);
        buf.extend_from_slice(&(MAX_RECORDS + 1).to_le_bytes()); // textures
        let r = parse_kn5(&buf, Kn5ParseOptions::default());
        assert!(!r.ok());
        assert!(r.error.contains("implausible texture count"), "{}", r.error);
    }

    #[test]
    fn texture_data_kept_only_when_requested() {
        let mut buf = header(4);
        buf.extend_from_slice(&1i32.to_le_bytes()); // textures
        buf.extend_from_slice(&1i32.to_le_bytes()); // active
        push_str(&mut buf, "body.dds");
        let dds = vec![9u8, 8, 7, 6];
        buf.extend_from_slice(&(dds.len() as u32).to_le_bytes());
        buf.extend_from_slice(&dds);
        buf.extend_from_slice(&0i32.to_le_bytes()); // materials
        base_node(&mut buf, "root", [0.0, 0.0, 0.0]);

        let skipped = parse_kn5(&buf, Kn5ParseOptions::default());
        assert!(skipped.ok(), "{}", skipped.error);
        assert_eq!(skipped.file.textures[0].data_size, 4);
        assert!(skipped.file.textures[0].data.is_empty());

        let kept = parse_kn5(
            &buf,
            Kn5ParseOptions {
                keep_texture_data: true,
            },
        );
        assert!(kept.ok(), "{}", kept.error);
        assert_eq!(kept.file.textures[0].data, dds);
    }

    #[test]
    fn inactive_texture_record_is_skipped() {
        let mut buf = header(4);
        buf.extend_from_slice(&2i32.to_le_bytes()); // textures
        buf.extend_from_slice(&0i32.to_le_bytes()); // active = 0 -> flag only
        buf.extend_from_slice(&1i32.to_le_bytes()); // active
        push_str(&mut buf, "body.dds");
        buf.extend_from_slice(&0u32.to_le_bytes()); // zero-length payload
        buf.extend_from_slice(&0i32.to_le_bytes()); // materials
        base_node(&mut buf, "root", [0.0, 0.0, 0.0]);

        let r = parse_kn5(&buf, Kn5ParseOptions::default());
        assert!(r.ok(), "{}", r.error);
        assert_eq!(r.file.textures.len(), 1);
        assert_eq!(r.file.textures[0].name, "body.dds");
    }

    #[test]
    fn trailing_bytes_after_tree_are_tolerated() {
        let mut buf = minimal_v4();
        buf.extend_from_slice(b"trailing junk appended by a mod exporter");
        let r = parse_kn5(&buf, Kn5ParseOptions::default());
        assert!(r.ok(), "{}", r.error);
        assert_eq!(r.file.nodes.len(), 1);
    }

    #[test]
    fn skinned_mesh_is_accepted() {
        let mut buf = header(4);
        buf.extend_from_slice(&0i32.to_le_bytes()); // textures
        buf.extend_from_slice(&0i32.to_le_bytes()); // materials
        buf.extend_from_slice(&3i32.to_le_bytes()); // SkinnedMesh
        push_str(&mut buf, "driver");
        buf.extend_from_slice(&0i32.to_le_bytes()); // children
        buf.push(1);
        buf.extend_from_slice(&[0u8; 3]); // flags
        buf.extend_from_slice(&1i32.to_le_bytes()); // bones
        push_str(&mut buf, "bone0");
        buf.extend_from_slice(&[0u8; MATRIX_BYTES]);
        buf.extend_from_slice(&0u32.to_le_bytes()); // vertices
        buf.extend_from_slice(&0u32.to_le_bytes()); // indices
        buf.extend_from_slice(&[0u8; SKINNED_TAIL_BYTES]);

        let r = parse_kn5(&buf, Kn5ParseOptions::default());
        assert!(r.ok(), "{}", r.error);
        assert_eq!(r.file.nodes[0].node_class, Kn5NodeClass::SkinnedMesh);
        assert!(!r.file.nodes[0].has_mesh);
    }

    #[test]
    fn matrix_multiply_matches_row_vector_convention() {
        // a = translation by (1,2,3); b = identity -> a * b == a.
        let mut a = IDENTITY_MATRIX;
        a[12] = 1.0;
        a[13] = 2.0;
        a[14] = 3.0;
        assert_eq!(multiply_matrices(a, IDENTITY_MATRIX), a);

        // Two translations compose by summing in this convention.
        let mut c = IDENTITY_MATRIX;
        c[12] = 10.0;
        let m = multiply_matrices(a, c);
        assert_eq!(m[12], 11.0);
        assert_eq!(m[13], 2.0);
    }

    #[test]
    fn to_bevy_mesh_builds_attributes() {
        let mesh = Kn5Mesh {
            positions: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
            normals: vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
            uvs: vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
            indices: vec![0, 1, 2],
            ..Default::default()
        };
        let bevy_mesh = to_bevy_mesh(&mesh);
        assert!(bevy_mesh
            .attribute(bevy::render::mesh::Mesh::ATTRIBUTE_POSITION)
            .is_some());
        assert!(bevy_mesh
            .attribute(bevy::render::mesh::Mesh::ATTRIBUTE_NORMAL)
            .is_some());
        assert!(bevy_mesh
            .attribute(bevy::render::mesh::Mesh::ATTRIBUTE_UV_0)
            .is_some());
        assert!(bevy_mesh.indices().is_some());
    }

    #[test]
    fn missing_file_reports_open_error() {
        let missing = std::env::temp_dir().join("racingedge_absent.kn5");
        let r = parse_kn5_file(&missing, Kn5ParseOptions::default());
        assert!(!r.ok());
        assert!(r.error.contains("cannot open file"), "{}", r.error);
    }

    #[test]
    fn is_kn5_checks_magic() {
        assert!(is_kn5(&minimal_v4()));
        assert!(!is_kn5(b"KN5\0"));
        assert!(!is_kn5(b"abc"));
    }
}
