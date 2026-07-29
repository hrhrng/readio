//! Which speaker readio is allowed to talk through.
//!
//! The failure this exists to prevent is mundane and expensive: you unplug your
//! headphones, or they fall asleep and the system quietly re-routes to the
//! built-in speakers, and the next sentence of your book plays out loud to the
//! room. A whitelist makes that impossible — name the outputs you are happy to
//! be heard on, and anywhere else readio keeps reading in silence.
//!
//! Three decisions worth stating:
//!
//! - **An unknown device counts as a mismatch.** If the whitelist cannot be
//!   checked, the safe answer is silence: the cost of guessing wrong is a room
//!   hearing your book, not a missing feature.
//! - **The probe never runs on the UI thread.** Asking macOS what the output
//!   device is costs about 200 ms, which is six dropped frames. Probes run on a
//!   worker thread and the interface reads a cached answer.
//! - **Being muted is never silent about itself.** A gate that stops the audio
//!   without saying so is indistinguishable from a broken engine, so the app
//!   reports the device by name and offers the two commands that fix it.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};

/// An audio output the system knows about.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Device {
    /// Name as the operating system reports it, e.g. `杨的AirPods Pro #2`.
    pub name: String,
    /// How it is attached: `bluetooth`, `usb`, `builtin`, … Empty when unknown.
    /// Matched as well as the name, so a rule can be as coarse as "bluetooth
    /// only".
    pub transport: String,
    /// Where sound is going right now.
    pub is_default: bool,
}

impl Device {
    /// Everything a whitelist entry is matched against.
    fn haystack(&self) -> String {
        format!("{} {}", self.name, self.transport).to_lowercase()
    }

    /// The name, and how it is attached.
    pub fn label(&self) -> String {
        if self.transport.is_empty() {
            self.name.clone()
        } else {
            format!("{} ({})", self.name, self.transport)
        }
    }
}

/// What to do when the current output is not on the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mismatch {
    /// Keep reading, without sound. The default, and the point of the feature.
    #[default]
    Silence,
    /// Speak anyway, and just say so — for people who want a reminder rather
    /// than a gate.
    Play,
}

/// The whitelist, from `config.yaml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Output {
    /// Substrings of allowed device names or transports, matched
    /// case-insensitively. Empty means every device is allowed and no probing
    /// happens at all.
    pub allow: Vec<String>,
    /// Command whose first line of output is the current device name, for
    /// systems readio has no built-in answer for. Empty uses the built-in probe.
    pub query: String,
    /// Seconds between checks. Unplugging headphones should stop the audio
    /// quickly, but not at the cost of a subprocess every frame.
    pub poll: u64,
    pub on_mismatch: Mismatch,
}

impl Default for Output {
    fn default() -> Self {
        Self {
            allow: Vec::new(),
            query: String::new(),
            poll: 5,
            on_mismatch: Mismatch::Silence,
        }
    }
}

impl Output {
    /// Whether any gating happens. An empty whitelist is not a locked door.
    pub fn is_active(&self) -> bool {
        self.allow.iter().any(|rule| !rule.trim().is_empty())
    }

    /// Add a rule, keeping the list free of duplicates and blanks.
    pub fn allow_device(&mut self, rule: &str) -> bool {
        let rule = rule.trim();
        if rule.is_empty() {
            return false;
        }
        let existing = self
            .allow
            .iter()
            .any(|have| have.eq_ignore_ascii_case(rule));
        if existing {
            return false;
        }
        self.allow.push(rule.to_string());
        true
    }

    pub fn rules(&self) -> String {
        self.allow
            .iter()
            .filter(|rule| !rule.trim().is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Whether the current device may be spoken through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// No whitelist, or the device is on it.
    Allowed { device: Option<Device> },
    /// A real device, not on the list.
    Blocked { device: Device },
    /// The device could not be determined; treated as blocked.
    Unknown { reason: String },
    /// The first probe has not come back yet.
    Pending,
}

impl Verdict {
    pub fn permits_audio(&self, on_mismatch: Mismatch) -> bool {
        match self {
            Verdict::Allowed { .. } => true,
            // A pending first probe holds the audio for one poll interval
            // rather than risking a burst out of the wrong speaker.
            Verdict::Blocked { .. } | Verdict::Unknown { .. } | Verdict::Pending => {
                on_mismatch == Mismatch::Play
            }
        }
    }
}

/// Whether `device` satisfies the whitelist.
///
/// Substring matching, because device names are long, localised and carry
/// serial numbers: `airpods` should match `杨的AirPods Pro #2` without the
/// reader having to transcribe it exactly.
pub fn allowed(device: &Device, allow: &[String]) -> bool {
    let rules: Vec<String> = allow
        .iter()
        .map(|rule| rule.trim().to_lowercase())
        .filter(|rule| !rule.is_empty())
        .collect();
    if rules.is_empty() {
        return true;
    }
    let haystack = device.haystack();
    rules.iter().any(|rule| haystack.contains(rule))
}

/// The output sound is going to right now.
pub fn probe(query: &str) -> Result<Device> {
    let query = query.trim();
    if !query.is_empty() {
        return probe_command(query);
    }
    let devices = devices()?;
    devices
        .into_iter()
        .find(|device| device.is_default)
        .ok_or_else(|| anyhow!("no default audio output reported"))
}

/// Every output the system knows about, so the reader can pick one to allow.
///
/// A custom `query` command only names the current device, so in that case the
/// list is just that one entry — enough for `/device allow`.
pub fn list(query: &str) -> Result<Vec<Device>> {
    let query = query.trim();
    if !query.is_empty() {
        return probe_command(query).map(|device| vec![device]);
    }
    devices()
}

/// Run the reader's own query command and take its first non-empty line.
fn probe_command(query: &str) -> Result<Device> {
    let args = crate::voice::command::split_args(query);
    let (program, rest) = args
        .split_first()
        .ok_or_else(|| anyhow!("output.query is empty"))?;
    let out = std::process::Command::new(program)
        .args(rest)
        .output()
        .with_context(|| format!("cannot run {program}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let name = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default()
        .to_string();
    if name.is_empty() {
        return Err(anyhow!("{program} printed no device name"));
    }
    Ok(Device {
        name,
        transport: String::new(),
        is_default: true,
    })
}

#[cfg(target_os = "macos")]
fn devices() -> Result<Vec<Device>> {
    let out = std::process::Command::new("system_profiler")
        .args(["SPAudioDataType", "-json"])
        .output()
        .context("cannot run system_profiler")?;
    parse_macos(&String::from_utf8_lossy(&out.stdout))
}

#[cfg(target_os = "linux")]
fn devices() -> Result<Vec<Device>> {
    let default = std::process::Command::new("pactl")
        .args(["get-default-sink"])
        .output()
        .context("cannot run pactl")?;
    let default = String::from_utf8_lossy(&default.stdout).trim().to_string();

    // `pactl list short sinks` prints one sink per line, name in column two.
    let sinks = std::process::Command::new("pactl")
        .args(["list", "short", "sinks"])
        .output()
        .context("cannot run pactl")?;
    let text = String::from_utf8_lossy(&sinks.stdout);
    let mut out: Vec<Device> = text
        .lines()
        .filter_map(|line| line.split('\t').nth(1))
        .map(|name| Device {
            name: name.to_string(),
            transport: String::new(),
            is_default: name == default,
        })
        .collect();
    if out.is_empty() && !default.is_empty() {
        out.push(Device {
            name: default,
            transport: String::new(),
            is_default: true,
        });
    }
    if out.is_empty() {
        return Err(anyhow!("pactl reported no sinks"));
    }
    Ok(out)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn devices() -> Result<Vec<Device>> {
    Err(anyhow!(
        "no built-in way to read audio devices here; set voice.output.query"
    ))
}

/// Pull the output devices out of `system_profiler SPAudioDataType -json`.
///
/// Entries with `coreaudio_device_output` are outputs; the current one carries
/// `coreaudio_default_audio_output_device`. The *input* flag looks almost
/// identical and usually sits on a different entry of the same headset, so
/// matching the exact key matters — picking the wrong one reports a device that
/// is not where the sound goes.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn parse_macos(json: &str) -> Result<Vec<Device>> {
    let root: serde_json::Value =
        serde_json::from_str(json).context("system_profiler did not return JSON")?;
    let mut out: Vec<Device> = Vec::new();
    visit(&root, &mut out);
    if out.is_empty() {
        return Err(anyhow!("no audio outputs in system_profiler output"));
    }
    Ok(out)
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn visit(node: &serde_json::Value, out: &mut Vec<Device>) {
    match node {
        serde_json::Value::Object(map) => {
            let is_default = map
                .get("coreaudio_default_audio_output_device")
                .and_then(|v| v.as_str())
                .is_some_and(|v| v.starts_with("spaudio_yes"));
            let has_output = map.contains_key("coreaudio_device_output");
            if (is_default || has_output)
                && let Some(name) = map.get("_name").and_then(|v| v.as_str())
            {
                let device = Device {
                    name: name.to_string(),
                    transport: transport_of(map),
                    is_default,
                };
                // The same headset appears once per direction; keep the entry
                // that knows it is the default.
                match out.iter_mut().find(|have| have.name == device.name) {
                    Some(have) if device.is_default => *have = device,
                    Some(_) => {}
                    None => out.push(device),
                }
            }
            for value in map.values() {
                visit(value, out);
            }
        }
        serde_json::Value::Array(items) => {
            for value in items {
                visit(value, out);
            }
        }
        _ => {}
    }
}

/// `coreaudio_device_type_bluetooth` → `bluetooth`.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn transport_of(map: &serde_json::Map<String, serde_json::Value>) -> String {
    map.get("coreaudio_device_transport")
        .and_then(|v| v.as_str())
        .map(|raw| {
            raw.rsplit_once("coreaudio_device_type_")
                .map(|(_, tail)| tail)
                .unwrap_or(raw)
                .to_string()
        })
        .unwrap_or_default()
}

/// A probe, injectable so the state machine can be tested without a sound card.
pub type Prober = Arc<dyn Fn() -> Result<Device> + Send + Sync>;

/// Keeps a current answer about the output device without ever blocking the
/// interface: probes run on a worker thread, the verdict is read from cache.
pub struct Gate {
    output: Output,
    prober: Prober,
    verdict: Verdict,
    inflight: Option<Receiver<Result<Device>>>,
    last_probe: Option<Instant>,
}

impl Gate {
    pub fn new(output: Output) -> Self {
        let query = output.query.clone();
        Self::with_prober(output, Arc::new(move || probe(&query)))
    }

    pub fn with_prober(output: Output, prober: Prober) -> Self {
        let mut gate = Self {
            output,
            prober,
            verdict: Verdict::Pending,
            inflight: None,
            last_probe: None,
        };
        gate.start_probe();
        gate
    }

    pub fn output(&self) -> &Output {
        &self.output
    }

    pub fn verdict(&self) -> &Verdict {
        &self.verdict
    }

    /// The device the last probe found, allowed or not.
    pub fn device(&self) -> Option<&Device> {
        match &self.verdict {
            Verdict::Allowed { device } => device.as_ref(),
            Verdict::Blocked { device } => Some(device),
            _ => None,
        }
    }

    /// True when audio is allowed out right now.
    pub fn permits_audio(&self) -> bool {
        self.verdict.permits_audio(self.output.on_mismatch)
    }

    /// Replace the whitelist — `/device allow` and `/device any` — and rule on
    /// the known device immediately, so the answer does not wait for a probe.
    pub fn set_allow(&mut self, allow: Vec<String>) {
        self.output.allow = allow;
        let device = self.device().cloned();
        self.verdict = match device {
            Some(device) if allowed(&device, &self.output.allow) => Verdict::Allowed {
                device: Some(device),
            },
            Some(device) => Verdict::Blocked { device },
            None if !self.output.is_active() => Verdict::Allowed { device: None },
            None => Verdict::Pending,
        };
        self.last_probe = None;
    }

    /// Probe again on the next poll, whatever the interval says.
    pub fn invalidate(&mut self) {
        self.last_probe = None;
    }

    /// Collect a finished probe and start the next one when it is due.
    ///
    /// Returns the verdict only when it changed, so a caller can report a
    /// device switch once instead of every frame.
    pub fn poll(&mut self) -> Option<Verdict> {
        let mut changed = None;

        if let Some(rx) = self.inflight.as_ref() {
            match rx.try_recv() {
                Ok(result) => {
                    self.inflight = None;
                    changed = self.apply(result);
                }
                Err(TryRecvError::Empty) => {}
                // The worker died; ask again next time rather than freezing.
                Err(TryRecvError::Disconnected) => self.inflight = None,
            }
        }

        let due = self
            .last_probe
            .is_none_or(|at| at.elapsed() >= Duration::from_secs(self.output.poll.max(1)));
        if self.inflight.is_none() && due {
            self.start_probe();
        }
        changed
    }

    fn start_probe(&mut self) {
        if !self.output.is_active() {
            // No whitelist: nothing to check, and no subprocess to pay for.
            self.verdict = Verdict::Allowed { device: None };
            self.last_probe = Some(Instant::now());
            return;
        }
        let (tx, rx) = channel();
        let prober = Arc::clone(&self.prober);
        let spawned = std::thread::Builder::new()
            .name("readio-audio-device".to_string())
            .spawn(move || {
                let _ = tx.send(prober());
            });
        match spawned {
            Ok(_) => {
                self.inflight = Some(rx);
                self.last_probe = Some(Instant::now());
            }
            // Cannot spawn: probe inline so the gate still works, with a hitch.
            Err(_) => {
                let result = (self.prober)();
                self.apply(result);
            }
        }
    }

    fn apply(&mut self, result: Result<Device>) -> Option<Verdict> {
        self.last_probe = Some(Instant::now());
        let verdict = match result {
            Ok(device) if allowed(&device, &self.output.allow) => Verdict::Allowed {
                device: Some(device),
            },
            Ok(device) => Verdict::Blocked { device },
            Err(err) => Verdict::Unknown {
                reason: format!("{err:#}"),
            },
        };
        if verdict == self.verdict {
            return None;
        }
        self.verdict = verdict.clone();
        Some(verdict)
    }

    /// Block until the current probe returns. Only for `/device`, where the
    /// reader asked a direct question and expects an answer.
    pub fn refresh_blocking(&mut self) {
        self.inflight = None;
        let result = (self.prober)();
        self.apply(result);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MACOS: &str = r#"{
      "SPAudioDataType": [
        {
          "_items": [
            {
              "_name": "杨的AirPods Pro #2",
              "coreaudio_default_audio_input_device": "spaudio_yes",
              "coreaudio_device_input": 1,
              "coreaudio_device_transport": "coreaudio_device_type_bluetooth"
            },
            {
              "_name": "杨的AirPods Pro #2",
              "coreaudio_default_audio_output_device": "spaudio_yes",
              "coreaudio_device_transport": "coreaudio_device_type_bluetooth",
              "coreaudio_device_output": 2
            },
            {
              "_name": "MacBook Pro 扬声器",
              "coreaudio_device_transport": "coreaudio_device_type_builtin",
              "coreaudio_device_output": 2
            },
            {
              "_name": "麦克风",
              "coreaudio_device_input": 1
            }
          ],
          "_name": "coreaudio_device"
        }
      ]
    }"#;

    fn airpods() -> Device {
        Device {
            name: "杨的AirPods Pro #2".to_string(),
            transport: "bluetooth".to_string(),
            is_default: true,
        }
    }

    fn speakers() -> Device {
        Device {
            name: "MacBook Pro 扬声器".to_string(),
            transport: "builtin".to_string(),
            is_default: true,
        }
    }

    #[test]
    fn outputs_are_listed_and_the_current_one_is_marked() {
        let devices = parse_macos(MACOS).expect("parse");
        let names: Vec<&str> = devices.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["杨的AirPods Pro #2", "MacBook Pro 扬声器"],
            "inputs are not outputs, and a headset is listed once"
        );
        let current: Vec<&str> = devices
            .iter()
            .filter(|d| d.is_default)
            .map(|d| d.name.as_str())
            .collect();
        assert_eq!(current, vec!["杨的AirPods Pro #2"], "exactly one default");
        assert_eq!(devices[0].transport, "bluetooth");
        assert_eq!(devices[1].transport, "builtin");
    }

    #[test]
    fn the_input_device_is_not_mistaken_for_the_output() {
        let inputs_only = r#"{"_items":[{"_name":"麦克风","coreaudio_default_audio_input_device":"spaudio_yes"}]}"#;
        assert!(
            parse_macos(inputs_only).is_err(),
            "a report with no outputs should fail, not guess"
        );
    }

    #[test]
    fn junk_from_the_probe_is_an_error_not_a_panic() {
        assert!(parse_macos("").is_err());
        assert!(parse_macos("not json at all").is_err());
        assert!(parse_macos("{}").is_err());
        assert!(parse_macos("[]").is_err());
    }

    #[test]
    fn matching_is_forgiving_about_case_and_serial_numbers() {
        assert!(allowed(&airpods(), &["airpods".to_string()]));
        assert!(allowed(&airpods(), &["AirPods Pro".to_string()]));
        assert!(
            allowed(&airpods(), &["bluetooth".to_string()]),
            "a rule may name the transport instead of the device"
        );
        assert!(!allowed(&speakers(), &["airpods".to_string()]));
        assert!(
            !allowed(&speakers(), &["bluetooth".to_string()]),
            "built-in speakers are not bluetooth"
        );
        assert!(allowed(&speakers(), &[]), "no whitelist allows anything");
        assert!(
            allowed(&speakers(), &["  ".to_string()]),
            "a whitelist of blank entries is not a whitelist"
        );
    }

    #[test]
    fn silence_is_the_default_answer_to_a_mismatch() {
        let blocked = Verdict::Blocked { device: speakers() };
        assert!(!blocked.permits_audio(Mismatch::Silence));
        assert!(blocked.permits_audio(Mismatch::Play));

        let unknown = Verdict::Unknown {
            reason: "no probe".to_string(),
        };
        assert!(
            !unknown.permits_audio(Mismatch::Silence),
            "an unchecked whitelist must not open the door"
        );
        assert!(
            !Verdict::Pending.permits_audio(Mismatch::Silence),
            "and neither should the moment before the first probe returns"
        );
        assert!(Verdict::Allowed { device: None }.permits_audio(Mismatch::Silence));
    }

    #[test]
    fn an_empty_whitelist_costs_nothing() {
        let mut gate = Gate::with_prober(
            Output::default(),
            Arc::new(|| panic!("must not probe without a whitelist")),
        );
        assert!(matches!(gate.verdict(), Verdict::Allowed { .. }));
        assert!(gate.permits_audio());
        assert_eq!(gate.poll(), None, "and no verdict ever changes");
    }

    #[test]
    fn switching_to_an_unlisted_device_flips_the_verdict_once() {
        let current = Arc::new(std::sync::Mutex::new(airpods()));
        let seen = Arc::clone(&current);
        let output = Output {
            allow: vec!["airpods".to_string()],
            poll: 0,
            ..Output::default()
        };
        let mut gate = Gate::with_prober(
            output,
            Arc::new(move || Ok(seen.lock().expect("lock").clone())),
        );

        // First probe: allowed.
        let verdict = wait_for_change(&mut gate);
        assert!(matches!(verdict, Verdict::Allowed { .. }), "{verdict:?}");
        assert!(gate.permits_audio());
        assert_eq!(gate.poll(), None, "a steady device reports nothing new");

        // The headphones go to sleep and the system falls back to speakers.
        *current.lock().expect("lock") = speakers();
        let verdict = wait_for_change(&mut gate);
        match verdict {
            Verdict::Blocked { device } => assert_eq!(device.name, speakers().name),
            other => panic!("expected a block, got {other:?}"),
        }
        assert!(!gate.permits_audio(), "audio must stop");
        assert_eq!(
            gate.poll(),
            None,
            "and the reader is told once, not every frame"
        );

        // Plugging them back in resumes without any command.
        *current.lock().expect("lock") = airpods();
        let verdict = wait_for_change(&mut gate);
        assert!(matches!(verdict, Verdict::Allowed { .. }), "{verdict:?}");
        assert!(gate.permits_audio());
    }

    #[test]
    fn allowing_the_current_device_takes_effect_without_waiting_for_a_probe() {
        let output = Output {
            allow: vec!["airpods".to_string()],
            poll: 0,
            ..Output::default()
        };
        let mut gate = Gate::with_prober(output, Arc::new(|| Ok(speakers())));
        let verdict = wait_for_change(&mut gate);
        assert!(matches!(verdict, Verdict::Blocked { .. }), "{verdict:?}");

        gate.set_allow(vec!["airpods".to_string(), "扬声器".to_string()]);
        assert!(
            gate.permits_audio(),
            "answering the prompt should unmute immediately"
        );

        gate.set_allow(Vec::new());
        assert!(gate.permits_audio(), "clearing the list allows everything");
        assert!(!gate.output().is_active());
    }

    #[test]
    fn a_failing_probe_reports_why_and_stays_silent() {
        let output = Output {
            allow: vec!["airpods".to_string()],
            poll: 0,
            ..Output::default()
        };
        let mut gate = Gate::with_prober(output, Arc::new(|| Err(anyhow!("no sound card"))));
        let verdict = wait_for_change(&mut gate);
        match verdict {
            Verdict::Unknown { reason } => assert!(reason.contains("no sound card"), "{reason}"),
            other => panic!("expected unknown, got {other:?}"),
        }
        assert!(!gate.permits_audio());
    }

    #[test]
    fn a_custom_query_command_names_the_device() {
        let device = probe_command("echo 'Studio Display Speakers'").expect("probe");
        assert_eq!(device.name, "Studio Display Speakers");
        assert!(device.is_default);

        assert!(
            probe_command("echo ''").is_err(),
            "a command that prints nothing is a failed probe"
        );
        assert!(
            probe_command("readio-no-such-command-exists").is_err(),
            "a missing command is an error, not a silent allow"
        );
        assert_eq!(
            list("echo 'Studio Display Speakers'").expect("list").len(),
            1,
            "a custom query knows only the current device"
        );
    }

    /// Probes run on a worker thread; give it a moment to answer.
    fn wait_for_change(gate: &mut Gate) -> Verdict {
        for _ in 0..200 {
            if let Some(verdict) = gate.poll() {
                return verdict;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("the gate never reported a verdict");
    }
}
