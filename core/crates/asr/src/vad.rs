//! Energy VAD: keeps speech, drops silence and cuts long speech at its quietest frame.
//! Pure functions over samples; the engine is never called from here.

use std::ops::Range;

use crate::limits::SAMPLE_RATE;

const FRAME_MS: usize = 30;
/// Nothing quieter than this is speech, whatever the room sounds like.
const FLOOR_DBFS: f32 = -50.0;
/// Speech must clear the estimated noise floor by this much to count.
const NOISE_MARGIN_DB: f32 = 12.0;
/// Percent of frame levels taken as the noise floor; pauses are the quiet tail of speech.
const NOISE_PERCENTILE: usize = 10;
/// Speech edges are widened so word onsets and tails are not clipped.
const PAD_MS: usize = 200;
/// Bursts of voice shorter than this are clicks (the record button), not speech.
const MIN_VOICE_MS: usize = 150;
/// Pauses shorter than this stay inside one utterance.
const MERGE_GAP_MS: usize = 300;
/// Canary drops words from one pass past about 20 s, so chunks stay below this.
const MAX_CHUNK_MS: usize = 20_000;
/// A long chunk is cut in its last part, where a pause is most likely.
const CUT_WINDOW_MS: usize = 5_000;

const FRAME: usize = ms_to_samples(FRAME_MS);
const MIN_VOICE_FRAMES: usize = MIN_VOICE_MS / FRAME_MS;
const MERGE_GAP_FRAMES: usize = MERGE_GAP_MS / FRAME_MS;

const fn ms_to_samples(ms: usize) -> usize {
    ms * SAMPLE_RATE as usize / 1000
}

/// What the VAD decided for one buffer, with the levels it decided on.
pub(crate) struct Segmentation {
    /// Sample ranges of speech, in order, each at most `MAX_CHUNK_MS` long. Silence yields none.
    pub chunks: Vec<Range<usize>>,
    pub noise_floor_dbfs: f32,
    pub threshold_dbfs: f32,
}

pub(crate) fn segment(pcm: &[f32]) -> Segmentation {
    if pcm.is_empty() {
        return Segmentation {
            chunks: Vec::new(),
            noise_floor_dbfs: f32::NEG_INFINITY,
            threshold_dbfs: FLOOR_DBFS,
        };
    }
    let levels: Vec<f32> = pcm.chunks(FRAME).map(level_dbfs).collect();
    let noise_floor_dbfs = noise_floor(&levels);
    let threshold_dbfs = FLOOR_DBFS.max(noise_floor_dbfs + NOISE_MARGIN_DB);
    let chunks = padded_spans(&voiced_frames(&levels, threshold_dbfs), pcm.len())
        .into_iter()
        .flat_map(|span| split_long(span, &levels))
        .collect();
    Segmentation {
        chunks,
        noise_floor_dbfs,
        threshold_dbfs,
    }
}

/// Power in dBFS of a buffer; an empty one is silence (`-inf`), not NaN.
pub(crate) fn level_dbfs(frame: &[f32]) -> f32 {
    let power = frame.iter().map(|sample| sample * sample).sum::<f32>() / frame.len().max(1) as f32;
    10.0 * power.log10()
}

fn noise_floor(levels: &[f32]) -> f32 {
    let mut sorted = levels.to_vec();
    let index = sorted.len() * NOISE_PERCENTILE / 100;
    sorted.select_nth_unstable_by(index, f32::total_cmp);
    sorted[index]
}

/// Runs of loud frames, with pauses shorter than `MERGE_GAP_FRAMES` bridged. Short bursts go
/// first, so a click cannot bridge a pause to the speech after it.
fn voiced_frames(levels: &[f32], threshold: f32) -> Vec<Range<usize>> {
    let mut runs: Vec<Range<usize>> = Vec::new();
    for burst in loud_bursts(levels, threshold) {
        if burst.len() < MIN_VOICE_FRAMES {
            continue;
        }
        match runs.last_mut() {
            Some(run) if burst.start - run.end < MERGE_GAP_FRAMES => run.end = burst.end,
            _ => runs.push(burst),
        }
    }
    runs
}

/// Consecutive frames above the threshold, exactly as they are.
fn loud_bursts(levels: &[f32], threshold: f32) -> Vec<Range<usize>> {
    let mut bursts: Vec<Range<usize>> = Vec::new();
    for (index, &level) in levels.iter().enumerate() {
        if level < threshold {
            continue;
        }
        match bursts.last_mut() {
            Some(burst) if burst.end == index => burst.end = index + 1,
            _ => bursts.push(index..index + 1),
        }
    }
    bursts
}

/// Frame runs to sample spans, padded; padding can make neighbours overlap, so they join.
fn padded_spans(runs: &[Range<usize>], len: usize) -> Vec<Range<usize>> {
    let pad = ms_to_samples(PAD_MS);
    let mut spans: Vec<Range<usize>> = Vec::new();
    for run in runs {
        let start = (run.start * FRAME).saturating_sub(pad);
        let end = (run.end * FRAME + pad).min(len);
        match spans.last_mut() {
            Some(span) if start <= span.end => span.end = end,
            _ => spans.push(start..end),
        }
    }
    spans
}

fn split_long(span: Range<usize>, levels: &[f32]) -> Vec<Range<usize>> {
    let max = ms_to_samples(MAX_CHUNK_MS);
    let window = ms_to_samples(CUT_WINDOW_MS);
    let mut chunks = Vec::new();
    let mut start = span.start;
    while span.end - start > max {
        let cut = quietest_cut(levels, start + max - window, start + max);
        chunks.push(start..cut);
        start = cut;
    }
    chunks.push(start..span.end);
    chunks
}

/// The start of the quietest frame whose start lies in `from..=to`.
fn quietest_cut(levels: &[f32], from: usize, to: usize) -> usize {
    (from.div_ceil(FRAME)..=to / FRAME)
        .min_by(|&a, &b| levels[a].total_cmp(&levels[b]))
        .map_or(to, |frame| frame * FRAME)
}

#[cfg(test)]
mod tests {
    use super::{ms_to_samples, segment, split_long, FRAME};
    use crate::signal::{noise, tone};

    const RATE: usize = 16_000;

    #[test]
    fn silence_has_no_speech() {
        assert!(segment(&[0.0; RATE * 2]).chunks.is_empty());
        assert!(segment(&[]).chunks.is_empty());
    }

    #[test]
    fn low_noise_does_not_trigger() {
        // Uniform noise with amplitude 0.0017 is about -60 dBFS, under the absolute floor.
        assert!(segment(&noise(3.0, 0.0017)).chunks.is_empty());
    }

    #[test]
    fn a_pause_splits_speech_into_two_chunks() {
        let mut pcm = tone(1.0, 0.3);
        pcm.resize(pcm.len() + RATE, 0.0);
        pcm.extend(tone(1.0, 0.3));
        let chunks = segment(&pcm).chunks;
        assert_eq!(chunks.len(), 2);
        assert!(
            chunks[0].end < RATE + RATE / 2,
            "first chunk must stop inside the pause"
        );
        assert!(
            chunks[1].start > RATE,
            "second chunk must start inside or after the pause"
        );
    }

    #[test]
    fn a_click_alone_is_not_speech() {
        let mut pcm = tone(0.06, 0.3);
        pcm.resize(RATE, 0.0);
        assert!(segment(&pcm).chunks.is_empty());
    }

    #[test]
    fn a_click_does_not_pull_in_the_speech_after_it() {
        let mut pcm = tone(0.06, 0.3);
        pcm.resize(pcm.len() + RATE / 5, 0.0);
        pcm.extend(tone(2.0, 0.3));
        pcm.resize(pcm.len() + 2 * RATE, 0.0);
        let chunks = segment(&pcm).chunks;
        assert_eq!(chunks.len(), 1);
        assert_eq!(
            chunks[0].start,
            ms_to_samples(40),
            "the click must not bridge the pause to the tone"
        );
    }

    #[test]
    fn a_short_word_is_still_speech() {
        let mut pcm = tone(0.25, 0.3);
        pcm.resize(RATE, 0.0);
        assert_eq!(segment(&pcm).chunks.len(), 1);
    }

    #[test]
    fn long_speech_is_cut_at_the_quietest_frame_of_the_last_window() {
        let max = ms_to_samples(20_000);
        let mut levels = vec![0.0_f32; max * 2 / FRAME];
        levels[17_000 * RATE / 1000 / FRAME] = -40.0;
        let quietest = 19_000 * RATE / 1000 / FRAME;
        levels[quietest] = -60.0;
        levels[5_000 * RATE / 1000 / FRAME] = -70.0;
        let chunks = split_long(0..max * 2, &levels);
        assert_eq!(chunks[0], 0..quietest * FRAME);
        assert!(chunks.iter().all(|chunk| chunk.len() <= max));
        assert_eq!(chunks.last().unwrap().end, max * 2);
    }
}
