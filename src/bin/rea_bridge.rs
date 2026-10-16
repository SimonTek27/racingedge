//! CLI tools that bridge REA reverse engineering analysis and FMOD bank
//! inspection to the racingedge modding suite.
//!
//! Usage:
//!   rea_bridge analyze-native-binary <path> [--output <fmt>]
//!   rea_bridge generate-rust-code <pseudocode-file> [--output <file>]
//!   rea_bridge parse-fmod-bank <bank> <header-info> [--output <file>]
//!   rea_bridge fsb-info <bank.fsb>
//!   rea_bridge fsb-extract <bank.fsb> <outdir>
//!   rea_bridge fsb-ogg <bank.fsb> <outdir>
//!   rea_bridge fsb-pack <out.fsb> <input.wav> [<input.wav> ...]
//!   rea_bridge fsb-import-ogg <out.fsb> <input.ogg> [--name <name>]
//!   rea_bridge fsb-encode <out.fsb> <input.wav> [--name <name>] [--quality <0-100>]

use racingedge::editor::audio::vorbis;
use racingedge::editor::audio::vorbis_encode;
use racingedge::editor::audio::{BankEditor, Fsb5, Fsb5Builder, SoundFormat, Wav};

fn main() {
    let args = std::env::args().collect::<Vec<_>>();

    if args.len() < 2 {
        print_usage();
        std::process::exit(1);
    }

    match args[1].as_str() {
        "analyze-native-binary" => analyze_native_binary(&args),
        "generate-rust-code" => generate_rust_code(&args),
        "parse-fmod-bank" => parse_fmod_bank(&args),
        "fsb-info" => fsb_info(&args),
        "fsb-extract" => fsb_extract(&args),
        "fsb-ogg" => fsb_ogg(&args),
        "fsb-pack" => fsb_pack(&args),
        "fsb-edit" => fsb_edit(&args),
        "fsb-import-ogg" => fsb_import_ogg(&args),
        "fsb-encode" => fsb_encode(&args),
        "--help" | "-h" | "help" => print_usage(),
        other => {
            eprintln!("Error: unknown command '{other}'");
            print_usage();
            std::process::exit(1);
        }
    }
}

fn print_usage() {
    println!("racingedge rea_bridge — REA + FMOD bank tooling");
    println!();
    println!("Commands:");
    println!(
        "  analyze-native-binary <path>     Show the REA analysis workflow for a native binary"
    );
    println!("  generate-rust-code <file>        Translate REA pseudocode into a Rust skeleton");
    println!("  parse-fmod-bank <bank> <header>  Plan an FSB parse from REA header info");
    println!("  fsb-info <bank.fsb>              Parse an FSB5 bank and print its samples");
    println!("  fsb-extract <bank.fsb> <outdir>  Extract samples (WAV for PCM, Ogg for Vorbis)");
    println!("  fsb-ogg <bank.fsb> <outdir>      Rebuild Vorbis samples as playable .ogg files");
    println!(
        "  fsb-pack <out.fsb> <wav...>      Build an FSB5 bank from one or more PCM .wav files"
    );
    println!("  fsb-edit <in.fsb> <out.fsb>      Edit a bank (rename, loop, rate, replace audio)");
    println!("  fsb-import-ogg <out.fsb> <in.ogg>  Build a Vorbis bank from an .ogg file");
    println!("  fsb-encode <out.fsb> <in.wav>    Encode a .wav to a Vorbis bank (FMOD encoder)");
}

fn analyze_native_binary(args: &[String]) {
    if args.len() < 3 {
        eprintln!("Error: missing binary path");
        std::process::exit(1);
    }
    let binary_path = &args[2];
    let output = flag_value(args, "--output").unwrap_or("json");

    println!("=== REA Native Binary Analysis ===");
    println!("Binary path: {binary_path}");
    println!("Output format: {output}");
    println!();
    println!("Run this command to produce the analysis:");
    println!("  rea analyze \"{binary_path}\" --format {output}");
    println!();
    println!("Key findings to collect for the audio editor:");
    println!("  - Procedure names/addresses for sample decode and channel setup");
    println!("  - String literals describing FSB fields, codecs and DSP effects");
    println!("  - Call graph edges from the mixer update to per-channel processing");
}

fn generate_rust_code(args: &[String]) {
    if args.len() < 3 {
        eprintln!("Error: missing pseudocode file");
        std::process::exit(1);
    }
    let pseudocode_file = &args[2];
    let output_file = flag_value(args, "--output").unwrap_or("generated.rs");

    println!("=== Generate Rust Code from REA Pseudocode ===");
    println!("Pseudocode file: {pseudocode_file}");
    println!("Output file: {output_file}");

    match std::fs::read_to_string(pseudocode_file) {
        Ok(content) => {
            let rust_code = rea_to_rust_audio_code(&content);
            match std::fs::write(output_file, &rust_code) {
                Ok(()) => println!("Rust code written to {output_file}"),
                Err(error) => eprintln!("Failed to write output file: {error}"),
            }
        }
        Err(error) => eprintln!("Failed to read pseudocode file: {error}"),
    }
}

fn parse_fmod_bank(args: &[String]) {
    if args.len() < 4 {
        eprintln!("Error: expected <bank> <header-info>");
        std::process::exit(1);
    }
    let fsb_path = &args[2];
    let header_info = &args[3];
    let output_file = flag_value(args, "--output").unwrap_or("fmod_parser.rs");

    println!("=== Parse FMOD Sound Bank from REA structures ===");
    println!("FSB path: {fsb_path}");
    println!("Header info: {header_info}");
    println!("Output module: {output_file}");
    println!();
    println!("Steps performed by the generated parser:");
    println!("  1. Validate the FSB5 magic and header against the REA layout");
    println!("  2. Walk each sample header using the 64-bit packed fields");
    println!("  3. Decode metadata chunks (channels, frequency, loop, codec data)");
    println!("  4. Slice sample payloads from the data section");
}

fn fsb_info(args: &[String]) {
    if args.len() < 3 {
        eprintln!("Error: missing .fsb path");
        std::process::exit(1);
    }
    let path = &args[2];

    let bank = match Fsb5::from_file(path) {
        Ok(bank) => bank,
        Err(error) => {
            eprintln!("Error: {error}");
            std::process::exit(1);
        }
    };

    println!("=== FSB5 bank: {path} ===");
    println!("version            : {}", bank.version);
    println!("num samples        : {}", bank.num_samples);
    println!("sample header size : {} bytes", bank.sample_header_size);
    println!("name table size    : {} bytes", bank.name_table_size);
    println!("data size          : {} bytes", bank.data_size);
    println!(
        "format             : {} (mode {})",
        bank.format,
        bank.format.as_u32()
    );
    if bank.format == SoundFormat::Vorbis {
        println!("note               : Vorbis payloads are Ogg-style packets (see libfsbvorbis64)");
    }
    println!();

    for sample in &bank.samples {
        let data_len = bank.sample_data(sample.index).map(|d| d.len()).unwrap_or(0);
        println!(
            "[{index:>3}] {name:<24} {freq:>6} Hz  {channels} ch  {frames:>8} frames  {duration:.3}s  {bytes:>7} bytes",
            index = sample.index,
            name = sample.name,
            freq = sample.frequency,
            channels = sample.channels,
            frames = sample.num_frames,
            duration = sample.duration_seconds(),
            bytes = data_len,
        );
        if sample.is_looped() {
            println!(
                "        loop: {}..{}",
                sample.loop_start.unwrap_or(0),
                sample.loop_end.unwrap_or(0)
            );
        }
        if let Some(vorbis) = sample.vorbis_data() {
            let packet_count = bank
                .vorbis_packets(sample.index)
                .map(Iterator::count)
                .unwrap_or(0);
            println!(
                "        vorbis: setup id {}  packets {}  seek entries {}",
                vorbis.setup_crc32,
                packet_count,
                vorbis.seek_table.len()
            );
        }
    }
}

fn fsb_extract(args: &[String]) {
    if args.len() < 4 {
        eprintln!("Error: expected <bank.fsb> <outdir>");
        std::process::exit(1);
    }
    let path = &args[2];
    let out_dir = &args[3];

    let bank = match Fsb5::from_file(path) {
        Ok(bank) => bank,
        Err(error) => {
            eprintln!("Error: {error}");
            std::process::exit(1);
        }
    };

    if let Err(error) = std::fs::create_dir_all(out_dir) {
        eprintln!("Error: could not create output directory: {error}");
        std::process::exit(1);
    }

    let extension = bank.format.file_extension();
    let mut written = 0usize;
    for sample in &bank.samples {
        let Some(data) = bank.sample_data(sample.index) else {
            eprintln!("warning: sample {} has no data, skipping", sample.index);
            continue;
        };

        // Pick the most usable container for the sample:
        //   PCM    -> RIFF/WAVE
        //   Vorbis -> Ogg Vorbis (when the setup header is known)
        //   other  -> raw codec payload
        let container = if let Some(wav) = bank.sample_to_wav(sample.index) {
            Some((wav, "wav"))
        } else if bank.format == SoundFormat::Vorbis {
            vorbis::build_ogg(&bank, sample.index)
                .ok()
                .map(|ogg| (ogg, "ogg"))
        } else {
            None
        };
        let (bytes, extension) = match container {
            Some(pair) => pair,
            None => (data.to_vec(), extension),
        };

        let file_name = format!(
            "{index:04}_{name}.{extension}",
            index = sample.index,
            name = sanitize_file_component(&sample.name),
        );
        let target = std::path::Path::new(out_dir).join(file_name);
        match std::fs::write(&target, &bytes) {
            Ok(()) => {
                written += 1;
                println!("wrote {} ({} bytes)", target.display(), bytes.len());
            }
            Err(error) => eprintln!("error: could not write {}: {error}", target.display()),
        }
    }

    println!("extracted {written} sample(s)");
}

fn fsb_ogg(args: &[String]) {
    if args.len() < 4 {
        eprintln!("Error: expected <bank.fsb> <outdir>");
        std::process::exit(1);
    }
    let path = &args[2];
    let out_dir = &args[3];

    let bank = match Fsb5::from_file(path) {
        Ok(bank) => bank,
        Err(error) => {
            eprintln!("Error: {error}");
            std::process::exit(1);
        }
    };

    if bank.format != SoundFormat::Vorbis {
        eprintln!(
            "Error: bank format is {} (mode {}), expected vorbis (15)",
            bank.format,
            bank.format.as_u32()
        );
        std::process::exit(1);
    }

    if let Err(error) = std::fs::create_dir_all(out_dir) {
        eprintln!("Error: could not create output directory: {error}");
        std::process::exit(1);
    }

    let mut written = 0usize;
    for sample in &bank.samples {
        match vorbis::build_ogg(&bank, sample.index) {
            Ok(ogg) => {
                let file_name = format!(
                    "{index:04}_{name}.ogg",
                    index = sample.index,
                    name = sanitize_file_component(&sample.name),
                );
                let target = std::path::Path::new(out_dir).join(file_name);
                match std::fs::write(&target, &ogg) {
                    Ok(()) => {
                        written += 1;
                        println!("wrote {} ({} bytes)", target.display(), ogg.len());
                    }
                    Err(error) => {
                        eprintln!("error: could not write {}: {error}", target.display())
                    }
                }
            }
            Err(error) => eprintln!("skipping sample {}: {error}", sample.index),
        }
    }

    println!("rebuilt {written} Ogg Vorbis file(s)");
}

fn fsb_pack(args: &[String]) {
    if args.len() < 4 {
        eprintln!("Error: expected <out.fsb> <input.wav> [<input.wav> ...]");
        std::process::exit(1);
    }
    let output = &args[2];
    let inputs = &args[3..];

    let mut builder: Option<Fsb5Builder> = None;
    let mut format: Option<SoundFormat> = None;

    for input in inputs {
        let bytes = match std::fs::read(input) {
            Ok(bytes) => bytes,
            Err(error) => {
                eprintln!("Error: could not read {input}: {error}");
                std::process::exit(1);
            }
        };
        let wav = match Wav::parse(&bytes) {
            Ok(wav) => wav,
            Err(error) => {
                eprintln!("Error: {input}: {error}");
                std::process::exit(1);
            }
        };
        let wav_format = match wav.sound_format() {
            Some(format) => format,
            None => {
                eprintln!(
                    "Error: {input}: unsupported WAV format ({}-bit, tag {})",
                    wav.bits_per_sample, wav.audio_format
                );
                std::process::exit(1);
            }
        };

        match format {
            None => {
                format = Some(wav_format);
                builder = Some(Fsb5Builder::new(wav_format));
            }
            Some(existing) if existing != wav_format => {
                eprintln!(
                    "Error: {input} is {wav_format} but the bank is already {existing}; all inputs must share one format"
                );
                std::process::exit(1);
            }
            _ => {}
        }

        let name = std::path::Path::new(input)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("sample")
            .to_string();
        if let Some(builder) = builder.as_mut() {
            builder.push(wav.into_pcm_sample(name));
        }
    }

    let builder = builder.expect("at least one input was parsed");
    match builder.build() {
        Ok(bytes) => match std::fs::write(output, &bytes) {
            Ok(()) => println!(
                "wrote {} ({} sample(s), {} bytes, {})",
                output,
                builder.len(),
                bytes.len(),
                format.expect("format set"),
            ),
            Err(error) => {
                eprintln!("Error: could not write {output}: {error}");
                std::process::exit(1);
            }
        },
        Err(error) => {
            eprintln!("Error: {error}");
            std::process::exit(1);
        }
    }
}

fn fsb_import_ogg(args: &[String]) {
    if args.len() < 4 {
        eprintln!("Error: expected <out.fsb> <in.ogg> [--name <name>]");
        std::process::exit(1);
    }
    let output = &args[2];
    let input = &args[3];

    let mut name: Option<String> = None;
    let mut position = 4usize;
    while position < args.len() {
        match args[position].as_str() {
            "--name" => match args.get(position + 1) {
                Some(value) => name = Some(value.clone()),
                None => {
                    eprintln!("Error: --name requires a value");
                    std::process::exit(1);
                }
            },
            other => {
                eprintln!("Error: unknown option '{other}'");
                std::process::exit(1);
            }
        }
        position += 2;
    }

    let bytes = std::fs::read(input).unwrap_or_else(|error| {
        eprintln!("Error: could not read {input}: {error}");
        std::process::exit(1);
    });

    let default_name = std::path::Path::new(input)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("sample")
        .to_string();
    let name = name.unwrap_or(default_name);

    match vorbis::import_ogg_to_bank(&bytes, &name) {
        Ok(bank) => match std::fs::write(output, &bank) {
            Ok(()) => println!(
                "wrote {} (1 Vorbis sample '{name}', {} bytes)",
                output,
                bank.len()
            ),
            Err(error) => {
                eprintln!("Error: could not write {output}: {error}");
                std::process::exit(1);
            }
        },
        Err(error) => {
            eprintln!("Error: {error}");
            std::process::exit(1);
        }
    }
}

fn fsb_encode(args: &[String]) {
    if args.len() < 4 {
        eprintln!("Error: expected <out.fsb> <in.wav> [--name <name>] [--quality <0-100>]");
        std::process::exit(1);
    }
    let output = &args[2];
    let input = &args[3];

    let mut name: Option<String> = None;
    let mut quality = vorbis_encode::DEFAULT_QUALITY;
    let mut position = 4usize;
    while position < args.len() {
        match args[position].as_str() {
            "--name" => match args.get(position + 1) {
                Some(value) => name = Some(value.clone()),
                None => {
                    eprintln!("Error: --name requires a value");
                    std::process::exit(1);
                }
            },
            "--quality" => match args.get(position + 1).map(|value| value.parse::<f32>()) {
                Some(Ok(percent)) if (0.0..=100.0).contains(&percent) => {
                    quality = percent / 100.0;
                }
                _ => {
                    eprintln!("Error: --quality expects a number from 0 to 100");
                    std::process::exit(1);
                }
            },
            other => {
                eprintln!("Error: unknown option '{other}'");
                std::process::exit(1);
            }
        }
        position += 2;
    }

    let bytes = std::fs::read(input).unwrap_or_else(|error| {
        eprintln!("Error: could not read {input}: {error}");
        std::process::exit(1);
    });
    let wav = Wav::parse(&bytes).unwrap_or_else(|error| {
        eprintln!("Error: {input}: {error}");
        std::process::exit(1);
    });
    if wav.sample_rate < 16_000 {
        eprintln!(
            "Error: Vorbis encoding needs a sample rate of at least 16000 Hz (got {})",
            wav.sample_rate
        );
        std::process::exit(1);
    }

    let channels = wav.channels as u32;
    let sample_rate = wav.sample_rate;
    let pcm = wav.to_f32_samples();
    if pcm.is_empty() {
        eprintln!("Error: {input}: WAV payload could not be decoded");
        std::process::exit(1);
    }

    let default_name = std::path::Path::new(input)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("sample")
        .to_string();
    let name = name.unwrap_or(default_name);

    match vorbis_encode::encode_to_bank(&pcm, channels, sample_rate, quality, &name) {
        Ok(bank) => match std::fs::write(output, &bank) {
            Ok(()) => println!(
                "wrote {} (1 Vorbis sample '{name}', {} bytes, quality {:.0}%)",
                output,
                bank.len(),
                quality * 100.0
            ),
            Err(error) => {
                eprintln!("Error: could not write {output}: {error}");
                std::process::exit(1);
            }
        },
        Err(error) => {
            eprintln!("Error: {error}");
            std::process::exit(1);
        }
    }
}

fn fsb_edit(args: &[String]) {
    if args.len() < 4 {
        eprintln!("Error: expected <in.fsb> <out.fsb> [options]");
        eprintln!("Options:");
        eprintln!("  --rename <index>=<name>          Rename a sample");
        eprintln!("  --freq <index>=<hz>              Change a sample's rate");
        eprintln!("  --loop <index>=<start>[:<end>]   Set (or update) a loop region");
        eprintln!("  --clear-loop <index>             Remove a loop region");
        eprintln!("  --replace <index>=<file.wav>     Replace a PCM sample's audio");
        std::process::exit(1);
    }
    let input = &args[2];
    let output = &args[3];

    let bank = match Fsb5::from_file(input) {
        Ok(bank) => bank,
        Err(error) => {
            eprintln!("Error: {error}");
            std::process::exit(1);
        }
    };
    let format = bank.format;
    let mut editor = BankEditor::from_bank(&bank);
    let mut changes = 0usize;

    let mut position = 4usize;
    while position < args.len() {
        let flag = args[position].clone();
        let value = args.get(position + 1).cloned();
        match flag.as_str() {
            "--rename" => {
                let (index, name) = parse_assignment(&flag, value.as_deref());
                if !editor.rename(index, name) {
                    eprintln!("Error: --rename: no sample at index {index}");
                    std::process::exit(1);
                }
                changes += 1;
            }
            "--freq" => {
                let (index, raw) = parse_assignment(&flag, value.as_deref());
                let hertz = raw.parse::<u32>().unwrap_or_else(|_| {
                    eprintln!("Error: --freq: '{raw}' is not a valid frequency");
                    std::process::exit(1);
                });
                if !editor.set_frequency(index, hertz) {
                    eprintln!("Error: --freq: no sample at index {index}");
                    std::process::exit(1);
                }
                changes += 1;
            }
            "--loop" => {
                let (index, raw) = parse_assignment(&flag, value.as_deref());
                let (start, end) = match raw.split_once(':') {
                    Some((start, end)) => (
                        start.parse::<u32>().unwrap_or(0),
                        end.parse::<u32>().unwrap_or(0),
                    ),
                    None => {
                        let start = raw.parse::<u32>().unwrap_or(0);
                        let frames = editor
                            .sample(index)
                            .map(|sample| sample.num_frames)
                            .unwrap_or(0);
                        (start, frames)
                    }
                };
                if !editor.set_loop(index, start, end) {
                    eprintln!("Error: --loop: no sample at index {index}");
                    std::process::exit(1);
                }
                changes += 1;
            }
            "--clear-loop" => {
                let raw = value.clone().unwrap_or_default();
                let index = raw.parse::<usize>().unwrap_or_else(|_| {
                    eprintln!("Error: --clear-loop expects a sample index, got '{raw}'");
                    std::process::exit(1);
                });
                if !editor.clear_loop(index) {
                    eprintln!("Error: --clear-loop: no sample at index {index}");
                    std::process::exit(1);
                }
                changes += 1;
            }
            "--replace" => {
                let (index, path) = parse_assignment(&flag, value.as_deref());
                if format.bytes_per_sample().is_none() {
                    eprintln!(
                        "Error: --replace only supports uncompressed banks (this bank is {format})"
                    );
                    std::process::exit(1);
                }
                let bytes = std::fs::read(&path).unwrap_or_else(|error| {
                    eprintln!("Error: could not read {path}: {error}");
                    std::process::exit(1);
                });
                let wav = match Wav::parse(&bytes) {
                    Ok(wav) => wav,
                    Err(error) => {
                        eprintln!("Error: {path}: {error}");
                        std::process::exit(1);
                    }
                };
                if wav.sound_format() != Some(format) {
                    eprintln!("Error: {path} does not match the bank format ({format})");
                    std::process::exit(1);
                }
                match editor.sample_mut(index) {
                    Some(sample) => {
                        sample.frequency = wav.sample_rate;
                        sample.channels = wav.channels as u32;
                    }
                    None => {
                        eprintln!("Error: --replace: no sample at index {index}");
                        std::process::exit(1);
                    }
                }
                editor.set_data(index, wav.data);
                changes += 1;
            }
            other => {
                eprintln!("Error: unknown option '{other}'");
                std::process::exit(1);
            }
        }
        position += 2;
    }

    match editor.build() {
        Ok(bytes) => match std::fs::write(output, &bytes) {
            Ok(()) => println!(
                "wrote {} ({changes} change(s), {} bytes)",
                output,
                bytes.len()
            ),
            Err(error) => {
                eprintln!("Error: could not write {output}: {error}");
                std::process::exit(1);
            }
        },
        Err(error) => {
            eprintln!("Error: {error}");
            std::process::exit(1);
        }
    }
}

/// Parse a `--flag <index>=<value>` argument.
fn parse_assignment(flag: &str, value: Option<&str>) -> (usize, String) {
    let value = value.unwrap_or_else(|| {
        eprintln!("Error: {flag} requires <index>=<value>");
        std::process::exit(1);
    });
    let (index, rest) = value.split_once('=').unwrap_or_else(|| {
        eprintln!("Error: {flag} expects <index>=<value>, got '{value}'");
        std::process::exit(1);
    });
    let index = index.parse::<usize>().unwrap_or_else(|_| {
        eprintln!("Error: {flag}: invalid sample index '{index}'");
        std::process::exit(1);
    });
    (index, rest.to_string())
}

/// Replace characters that are unsafe in a file name.
fn sanitize_file_component(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            other => other,
        })
        .collect();
    if cleaned.is_empty() {
        "sample".to_string()
    } else {
        cleaned
    }
}

fn flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter()
        .position(|arg| arg == flag)
        .and_then(|index| args.get(index + 1))
        .map(String::as_str)
}

/// Translate REA-discovered pseudocode into a Rust/Bevy skeleton.
fn rea_to_rust_audio_code(function_desc: &str) -> String {
    let mut result = String::new();
    result.push_str("// Generated from REA pseudocode analysis\n");
    result.push_str("// Source: REA native binary analysis\n\n");

    let lowered = function_desc.to_lowercase();
    if lowered.contains("play") || lowered.contains("sample") {
        result.push_str("// FMOD sample playback -> Bevy audio equivalent\n");
        result.push_str("// BEFORE (pseudocode from REA):\n");
        result.push_str("//   void PlaySample(FmodSample* sample, FmodChannel* channel) {\n");
        result.push_str("//       sample->Play(channel->volume, channel->pitch);\n");
        result.push_str("//       if (channel->dsp) { ApplyDSP(channel->dsp); }\n");
        result.push_str("//   }\n\n");
        result.push_str("// AFTER (Rust/Bevy):\n");
        result.push_str("// audio_source.volume = channel_params.volume_curve.first().copied().unwrap_or(1.0);\n");
        result.push_str("// audio_source.pitch  = channel_params.pitch_curve.first().copied().unwrap_or(1.0);\n");
        result.push_str("// audio_source.looped = sample_info.loop_type != \"one_shot\";\n");
    } else {
        result.push_str("// No automatic translation matched; review the pseudocode manually.\n");
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_code_for_play_functions() {
        let code = rea_to_rust_audio_code("void PlaySample(FmodSample* sample)");
        assert!(code.contains("volume"));
        assert!(code.contains("pitch"));
    }

    #[test]
    fn leaves_unrelated_functions_alone() {
        let code = rea_to_rust_audio_code("int compute_friction(int x)");
        assert!(!code.contains("audio_source.volume"));
    }

    #[test]
    fn parses_flag_values() {
        let args = vec![
            "rea_bridge".to_string(),
            "fsb-info".to_string(),
            "b.fsb".to_string(),
        ];
        assert_eq!(flag_value(&args, "--output"), None);

        let args = vec![
            "rea_bridge".to_string(),
            "generate-rust-code".to_string(),
            "in.txt".to_string(),
            "--output".to_string(),
            "out.rs".to_string(),
        ];
        assert_eq!(flag_value(&args, "--output"), Some("out.rs"));
    }
}
