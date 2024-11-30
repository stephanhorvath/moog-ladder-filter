use nih_plug::prelude::*;
use parking_lot::Mutex;
use std::sync::Arc;

struct MoogLadderFilter {
    params: Arc<FilterParams>,
    prev_outputs: Vec<f32>,
    prev_w: Vec<f32>,
    g: f32,
}

#[derive(Params)]
struct FilterParams {
    #[id = "cutoff"]
    pub cutoff: FloatParam,
    #[id = "resonance"]
    pub resonance: FloatParam,
    #[id = "drive"]
    pub drive: FloatParam,
    #[id = "output"]
    pub output: FloatParam,
    #[id = "attack"]
    pub attack: FloatParam,
    #[id = "release"]
    pub release: FloatParam,
    #[id = "amount"]
    pub amount: FloatParam,
    #[id = "pole"]
    pub two_pole_four_pole: BoolParam,
    #[id = "pass"]
    pub hi_low_pass: BoolParam,
}

impl Default for MoogLadderFilter {
    fn default() -> Self {
        Self {
            params: Arc::new(FilterParams::default()),
            prev_outputs: vec![0.0, 0.0, 0.0, 0.0],
            prev_w: vec![0.0, 0.0, 0.0],
            g: 0.0,
        }
    }
}

impl Default for FilterParams {
    fn default() -> Self {
        Self {
            cutoff: FloatParam::new(
                "Cutoff",
                20_000.0,
                FloatRange::Skewed {
                    min: 20.0,
                    max: 20_000.0,
                    factor: 0.5,
                },
            )
            .with_smoother(SmoothingStyle::Exponential(1.0))
            .with_value_to_string(formatters::v2s_f32_hz_then_khz(2))
            .with_string_to_value(formatters::s2v_f32_hz_then_khz()),

            resonance: FloatParam::new("Resonance", 0.0, FloatRange::Linear { min: 0.0, max: 4.0 })
                .with_smoother(SmoothingStyle::Linear(10.0)),

            drive: FloatParam::new(
                "Drive",
                0.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 10.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(10.0))
            .with_value_to_string(formatters::v2s_f32_percentage(2))
            .with_string_to_value(formatters::s2v_f32_percentage()),

            output: FloatParam::new(
                "Output",
                1.0,
                FloatRange::Linear {
                    min: -10.0,
                    max: 10.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(10.0))
            .with_value_to_string(formatters::v2s_f32_percentage(2))
            .with_string_to_value(formatters::s2v_f32_percentage()),

            attack: FloatParam::new(
                "Attack",
                0.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 10.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(10.0))
            .with_value_to_string(formatters::v2s_f32_percentage(2))
            .with_string_to_value(formatters::s2v_f32_percentage()),

            release: FloatParam::new(
                "Release",
                0.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 10.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(10.0))
            .with_value_to_string(formatters::v2s_f32_percentage(2))
            .with_string_to_value(formatters::s2v_f32_percentage()),

            amount: FloatParam::new(
                "Amount",
                0.0,
                FloatRange::Linear {
                    min: -10.0,
                    max: 10.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(10.0))
            .with_value_to_string(formatters::v2s_f32_percentage(2))
            .with_string_to_value(formatters::s2v_f32_percentage()),

            two_pole_four_pole: BoolParam::new("2-Pole | 4-Pole", true),

            hi_low_pass: BoolParam::new("HP | LP", true),
        }
    }
}

impl Plugin for MoogLadderFilter {
    const VENDOR: &'static str = "SMC7";
    const NAME: &'static str = env!("CARGO_PKG_NAME");
    const VERSION: &'static str = env!("CARGO_PKG_NAME");
    const URL: &'static str = env!("CARGO_PKG_HOMEPAGE");
    const EMAIL: &'static str = "sescal24@student.aau.dk";

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),

            aux_input_ports: &[],
            aux_output_ports: &[],

            names: PortNames::const_default(),
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
            ..AudioIOLayout::const_default()
        },
    ];

    const MIDI_INPUT: MidiConfig = MidiConfig::None;

    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.g = 1.0
            - (-std::f32::consts::TAU * (self.params.cutoff.smoothed.next())
                / buffer_config.sample_rate as f32)
                .exp();
        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        for channel_samples in buffer.iter_samples() {
            let cutoff = self.params.cutoff.smoothed.next();
            let drive = self.params.drive.smoothed.next();
            let resonance = self.params.resonance.smoothed.next();
            let output = self.params.output.smoothed.next();

            let sample_rate = _context.transport().sample_rate;
            let inverse_sample_rate = 1.0 / sample_rate;

            self.g = 1.0 - (-std::f32::consts::TAU * (cutoff * inverse_sample_rate)).exp();
            let two_vt = 2.0 * 0.026;
            let two_vt_reciprocal = 1.0 / two_vt;
            let two_vt_g = two_vt * self.g as f32;

            for sample in channel_samples {
                let input = *sample;
                let tanh_stage_1 = (input
                    - ((4.0 * resonance * self.prev_outputs[3]) * two_vt_reciprocal) * drive)
                    .tanh();
                let stage_1 = self.prev_outputs[0] + two_vt_g * (tanh_stage_1 - self.prev_w[0]);
                self.prev_outputs[0] = stage_1;

                self.prev_w[0] = (stage_1 * two_vt_reciprocal * drive).tanh();

                let stage_2 = self.prev_outputs[1] + two_vt_g * (self.prev_w[0] - self.prev_w[1]);
                self.prev_outputs[1] = stage_2;

                self.prev_w[1] = (stage_2 * two_vt_reciprocal * drive).tanh();

                let stage_3 = self.prev_outputs[2] + two_vt_g * (self.prev_w[1] - self.prev_w[2]);
                self.prev_outputs[2] = stage_3;

                self.prev_w[2] = (stage_3 * two_vt_reciprocal * drive).tanh();

                let stage_4 = self.prev_outputs[3]
                    + two_vt_g
                        * (self.prev_w[2]
                            - (self.prev_outputs[3] * two_vt_reciprocal * drive).tanh());

                *sample = output * stage_4;
                self.prev_outputs[3] = stage_4;
            }
        }

        ProcessStatus::Normal
    }

    fn deactivate(&mut self) {}
}

impl ClapPlugin for MoogLadderFilter {
    const CLAP_ID: &'static str = "com.stephanhorvath.moogladderfilter";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("A Rust implementation of the Moog Ladder Filter");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Mono,
        ClapFeature::Utility,
    ];
}

impl Vst3Plugin for MoogLadderFilter {
    const VST3_CLASS_ID: [u8; 16] = *b"MoogLadderFilter";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Tools];
}

nih_export_clap!(MoogLadderFilter);
nih_export_vst3!(MoogLadderFilter);
