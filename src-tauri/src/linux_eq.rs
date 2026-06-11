//! Linux system equalizer integration.
//!
//! Linux does not have Equalizer APO.  The native system-wide path is
//! PipeWire's filter-chain module with the builtin `param_eq` filter.
//! `param_eq` can load AutoEQ/SquigLink parametric files with `Filter:`
//! lines, and it can also be configured with an inline list of biquad
//! filters.  For GraphicEQ-only AutoEQ entries we therefore synthesize a
//! conservative 31-band parametric approximation so the system EQ path is
//! still usable instead of stopping at "export only".

use std::fs::{self, File};
use std::io::Write;

use crate::state::{AppError, AppState};

const PIPEWIRE_CONF_NAME: &str = "99-smart-eq-preset-switcher-parametric-eq.conf";
const STANDARD_GRAPHIC_BANDS: [f64; 31] = [
    20.0, 25.0, 31.5, 40.0, 50.0, 63.0, 80.0, 100.0, 125.0, 160.0, 200.0, 250.0, 315.0,
    400.0, 500.0, 630.0, 800.0, 1000.0, 1250.0, 1600.0, 2000.0, 2500.0, 3150.0, 4000.0,
    5000.0, 6300.0, 8000.0, 10000.0, 12500.0, 16000.0, 20000.0,
];

fn pipewire_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn pipewire_conf_path() -> Result<std::path::PathBuf, AppError> {
    Ok(dirs::config_dir()
        .ok_or(AppError::AppDataUnavailable)?
        .join("pipewire")
        .join("pipewire.conf.d")
        .join(PIPEWIRE_CONF_NAME))
}

fn has_parametric_filter_lines(content: &str) -> bool {
    content
        .lines()
        .any(|line| line.trim_start().to_ascii_lowercase().starts_with("filter"))
}

fn parse_graphic_eq_points(content: &str) -> Vec<(f64, f64)> {
    let mut points = Vec::new();

    for line in content.lines() {
        let Some((command, payload)) = line.split_once(':') else {
            continue;
        };

        let command = command.trim().to_ascii_lowercase();
        if command != "graphiceq" && command != "graphicq" {
            continue;
        }

        for pair in payload.split(';') {
            let mut parts = pair.split_whitespace();
            let Some(freq_raw) = parts.next() else { continue; };
            let Some(gain_raw) = parts.next() else { continue; };
            let freq_raw = freq_raw.trim_end_matches("Hz");
            let gain_raw = gain_raw.trim_end_matches("dB");

            let Ok(freq) = freq_raw.parse::<f64>() else { continue; };
            let Ok(gain) = gain_raw.parse::<f64>() else { continue; };

            if freq.is_finite() && gain.is_finite() && freq > 0.0 {
                points.push((freq, gain));
            }
        }
    }

    points.sort_by(|left, right| left.0.partial_cmp(&right.0).unwrap_or(std::cmp::Ordering::Equal));
    points.dedup_by(|left, right| (left.0 - right.0).abs() < 0.001);
    points
}

fn interpolate_gain(points: &[(f64, f64)], freq: f64) -> f64 {
    if points.is_empty() {
        return 0.0;
    }

    if freq <= points[0].0 {
        return points[0].1;
    }

    if freq >= points[points.len() - 1].0 {
        return points[points.len() - 1].1;
    }

    let target = freq.log10();

    for window in points.windows(2) {
        let (f0, g0) = window[0];
        let (f1, g1) = window[1];

        if freq >= f0 && freq <= f1 {
            let x0 = f0.log10();
            let x1 = f1.log10();
            if (x1 - x0).abs() < f64::EPSILON {
                return g0;
            }
            let ratio = (target - x0) / (x1 - x0);
            return g0 + (g1 - g0) * ratio;
        }
    }

    0.0
}

fn format_frequency(freq: f64) -> String {
    if (freq.fract()).abs() < 0.001 {
        format!("{freq:.0}")
    } else {
        format!("{freq:.1}")
    }
}

fn graphic_eq_to_parametric_config(points: &[(f64, f64)]) -> Option<String> {
    if points.is_empty() {
        return None;
    }

    let mut gains: Vec<(f64, f64)> = STANDARD_GRAPHIC_BANDS
        .iter()
        .map(|freq| (*freq, interpolate_gain(points, *freq)))
        .filter(|(_, gain)| gain.abs() >= 0.05)
        .collect();

    if gains.is_empty() {
        gains.push((1000.0, 0.0));
    }

    let max_positive_gain = gains
        .iter()
        .map(|(_, gain)| *gain)
        .fold(0.0_f64, f64::max);
    let preamp = if max_positive_gain > 0.0 {
        -max_positive_gain
    } else {
        0.0
    };

    let mut output = String::new();
    output.push_str("# Converted from GraphicEQ by SmartEQPresetSwitcher.\n");
    output.push_str("# Approximation: 31 one-third-octave peaking filters, Q 4.318.\n");
    output.push_str(format!("Preamp: {:.1} dB\n", preamp).as_str());

    for (index, (freq, gain)) in gains.iter().enumerate() {
        output.push_str(
            format!(
                "Filter {}: ON PK Fc {} Hz Gain {:.1} dB Q 4.318\n",
                index + 1,
                format_frequency(*freq),
                gain
            )
            .as_str(),
        );
    }

    Some(output)
}

fn extract_active_preset_content() -> Result<Option<String>, AppError> {
    let state = AppState::initialize()?;
    let snapshot = {
        let mut guard = state.lock()?;
        guard.snapshot()?
    };

    for group in &snapshot.groups {
        if let Some(active) = &group.active_preset {
            if let Some(item) = group.presets.iter().find(|preset| &preset.name == active) {
                return Ok(Some(item.content.clone()));
            }
        }
    }

    Ok(None)
}

fn write_pipewire_filter_chain_config(
    pipewire_conf: &std::path::Path,
    parametric_config_path: &std::path::Path,
) -> Result<(), AppError> {
    if let Some(parent) = pipewire_conf.parent() {
        fs::create_dir_all(parent)?;
    }

    let active_file = pipewire_string(parametric_config_path.to_string_lossy().as_ref());
    let mut pipe_file = File::create(pipewire_conf)?;
    write!(
        pipe_file,
        r#"# Autogenerated by SmartEQPresetSwitcher.
# Restart PipeWire or log out/in after first setup:
#   systemctl --user restart pipewire pipewire-pulse wireplumber
#
# This creates a virtual sink backed by PipeWire filter-chain and the builtin
# param_eq filter. Route audio to "SmartEQPresetSwitcher EQ" in your desktop
# sound settings, or set it as default with:
#   pactl set-default-sink smart-eq-preset-switcher.eq

context.modules = [
  {{
    name = libpipewire-module-filter-chain
    args = {{
      node.description = "SmartEQPresetSwitcher EQ"
      media.name = "SmartEQPresetSwitcher EQ"
      filter.graph = {{
        nodes = [
          {{
            type = builtin
            name = eq
            label = param_eq
            config = {{
              filename = "{active_file}"
            }}
          }}
        ]
      }}
      capture.props = {{
        node.name = "smart-eq-preset-switcher.eq"
        node.description = "SmartEQPresetSwitcher EQ"
        media.class = "Audio/Sink"
        audio.channels = 2
        audio.position = [ FL FR ]
      }}
      playback.props = {{
        node.passive = true
        audio.channels = 2
        audio.position = [ FL FR ]
      }}
    }}
  }}
]
"#
    )?;

    Ok(())
}

/// Exports the currently active preset into Linux EQ files and writes a
/// PipeWire filter-chain setup file when the active preset can be represented
/// as parametric EQ. Native `Filter:` presets are used directly; GraphicEQ-only
/// presets are converted to a 31-band parametric approximation.
pub fn export_active_preset() -> Result<(), AppError> {
    let base_dir = dirs::config_dir()
        .ok_or(AppError::AppDataUnavailable)?
        .join(crate::state::APP_FOLDER_NAME)
        .join("linux-eq");
    fs::create_dir_all(&base_dir)?;

    let raw_out = base_dir.join("active-equalizerapo.txt");
    let parametric_out = base_dir.join("active-parametric-eq.txt");
    let graphic_out = base_dir.join("active-graphiceq-converted-parametric.txt");
    let pipewire_conf = pipewire_conf_path()?;

    let Some(content) = extract_active_preset_content()? else {
        let _ = fs::remove_file(&raw_out);
        let _ = fs::remove_file(&parametric_out);
        let _ = fs::remove_file(&graphic_out);
        let _ = fs::remove_file(&pipewire_conf);
        return Ok(());
    };

    let mut raw_file = File::create(&raw_out)?;
    raw_file.write_all(content.as_bytes())?;

    if has_parametric_filter_lines(&content) {
        let mut parametric_file = File::create(&parametric_out)?;
        parametric_file.write_all(content.as_bytes())?;
        let _ = fs::remove_file(&graphic_out);
        write_pipewire_filter_chain_config(&pipewire_conf, &parametric_out)?;
        return Ok(());
    }

    let graphic_points = parse_graphic_eq_points(&content);
    if let Some(converted) = graphic_eq_to_parametric_config(&graphic_points) {
        let mut parametric_file = File::create(&parametric_out)?;
        parametric_file.write_all(converted.as_bytes())?;
        let mut graphic_file = File::create(&graphic_out)?;
        graphic_file.write_all(converted.as_bytes())?;
        write_pipewire_filter_chain_config(&pipewire_conf, &parametric_out)?;
        return Ok(());
    }

    let mut parametric_file = File::create(&parametric_out)?;
    parametric_file.write_all(b"# SmartEQPresetSwitcher could not derive a PipeWire-compatible parametric preset from the active preset.\n")?;
    let _ = fs::remove_file(&graphic_out);
    let _ = fs::remove_file(&pipewire_conf);

    Ok(())
}
