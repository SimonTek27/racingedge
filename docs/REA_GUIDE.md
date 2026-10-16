# Running REA against the stock authoring tool

[REA](https://github.com/morluto/rea) (“Reverse Engineer Anything”) drives a
native decompiler (Ghidra here) to analyse the authoring tool's binaries. This
guide is what produced the evidence in [`../REA_FINDINGS.md`](../REA_FINDINGS.md).

## Prerequisites

| Requirement | Notes |
| --- | --- |
| Node.js 22.x / 24.x / 26+ | `node --version` |
| REA CLI | runs directly via `npx -y rea-agents@latest` |
| Ghidra 12.x + a full JDK 21+ | must be on the machine |

Check everything at once:

```cmd
npx -y rea-agents@latest doctor
```

`doctor` prints a per-check table. `ghidra`, `ghidra-headless` and
`ghidra-java` must report `ok: true`.

## Commands

```cmd
:: overview of a binary (procedure + string counts, segments)
npx -y rea-agents@latest analyze "<binary>" --format json

:: search strings or procedures (regex)
npx -y rea-agents@latest search  "<binary>" --pattern "vorbis|sample|decode" --kind procedures --mode regex --format json

:: list capabilities / providers
npx -y rea-agents@latest capabilities
```

Useful targets inside the analysed build's install directory
(`C:\Program Files\FMOD SoundSystem\FMOD Studio 2.03.13`):

| Binary | Why |
| --- | --- |
| `libfsbvorbis64.dll` | the FSB Vorbis codec (fast, ~1 min) |
| `opus.dll`, `xmaencoder64.dll` | other FSB codecs |
| `fmodstudioui.dll` | authoring UI (**23 MB — exceeds the 5.5 min Ghidra budget; needs `--snapshot`) |

## Ready-made script

[`../tools/rea_analysis.cmd`](../tools/rea_analysis.cmd) runs the two searches
that produced the libvorbis inventory and writes the JSON evidence:

```cmd
tools\rea_analysis.cmd
```

Note: when calling `npx.cmd` from inside another `.cmd`, prefix it with `call`
or the parent batch terminates after the first command.

## Timeouts on large binaries

`fmodstudioui.dll` (23 MB) aborts with:

```json
{ "error": "Analysis failed", "code": "provider_timeout" }
```

The Ghidra analysis budget is ~330 s. For big binaries, use an incremental
**snapshot** so repeated runs reuse prior analysis:

```cmd
npx -y rea-agents@latest analyze "<binary>" --snapshot fmod-ui.snapshot --format json
```

## What we learned

See [`../REA_FINDINGS.md`](../REA_FINDINGS.md). In short: `libfsbvorbis64.dll`
is stock **libvorbis 1.3.2**, and the FSB5 container/packet layout is fully
characterised, which is what the Rust editor in this repo implements.
