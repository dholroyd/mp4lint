use serde::Serialize;
#[derive(Debug, Clone, Serialize)]
pub struct SdtpFlags {
    pub is_leading: u8,
    pub depends_on: u8,
    pub depended_on: u8,
    pub has_redundancy: u8,
}

#[derive(Debug, Clone, Serialize)]
pub struct EditListEntryInfo {
    pub segment_duration: u64,
    pub media_time: i64,
    pub media_rate: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SampleInfo {
    pub index: u32,
    pub presentation_index: u32,
    pub offset: u64,
    pub size: u32,
    pub decode_timestamp: u64,
    pub duration: u32,
    pub composition_offset: i64,
    pub is_sync: bool,
    pub chunk_index: u32,
    pub description_index: u32,
    pub stts_entry_index: u32,
    pub ctts_entry_index: Option<u32>,
    pub stsc_entry_index: u32,
    pub stss_entry_index: Option<u32>,
    pub uses_co64: bool,
    pub has_variable_sizes: bool,
    pub sdtp: Option<SdtpFlags>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SampleRange {
    pub first_sample: u32,
    pub last_sample: u32,
    pub start_time: i64,
    pub end_time: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct EditedRange {
    pub presentation_start: i64,
    pub presentation_end: i64,
    pub media_start: i64,
    pub media_end: i64,
    pub edit_index: u32,
    pub is_empty: bool,
    pub media_rate: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrackSampleTable {
    pub track_id: u32,
    pub handler_type: Option<String>,
    pub sample_count: u32,
    pub codec: Option<String>,

    pub stts_entries: Vec<(u32, u32)>,
    pub stsc_entries: Vec<(u32, u32, u32)>,
    pub chunk_offsets: Vec<u64>,
    pub uses_co64: bool,
    pub constant_sample_size: Option<u32>,
    pub sample_sizes: Vec<u32>,
    pub sync_samples: Option<Vec<u32>>,
    pub ctts_entries: Vec<(u32, i64)>,
    pub sdtp_flags: Vec<(u8, u8, u8, u8)>,
    pub edit_list: Vec<EditListEntryInfo>,

    pub dts_index: Vec<DtsIndexEntry>,
    pub chunk_index: Vec<ChunkIndexEntry>,
    pub ctts_prefix_sums: Vec<u32>,

    pub sample_ranges: Vec<SampleRange>,
    pub presentation_order: Vec<u32>,
    pub sorted_cts: Vec<i64>,
    pub cts_to_decode: Vec<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DtsIndexEntry {
    pub sample_index: u32,
    pub cumulative_dts: u64,
    pub stts_entry_idx: usize,
    pub samples_in_entry: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChunkIndexEntry {
    pub sample_index: u32,
    pub chunk_idx: u32,
    pub stsc_entry_idx: usize,
    pub sample_in_chunk: u32,
}

const INDEX_INTERVAL: u32 = 1000;

impl TrackSampleTable {
    pub fn new() -> Self {
        TrackSampleTable {
            track_id: 0,
            handler_type: None,
            sample_count: 0,
            codec: None,
            stts_entries: Vec::new(),
            stsc_entries: Vec::new(),
            chunk_offsets: Vec::new(),
            uses_co64: false,
            constant_sample_size: None,
            sample_sizes: Vec::new(),
            sync_samples: None,
            ctts_entries: Vec::new(),
            sdtp_flags: Vec::new(),
            edit_list: Vec::new(),
            dts_index: Vec::new(),
            chunk_index: Vec::new(),
            ctts_prefix_sums: Vec::new(),
            sample_ranges: Vec::new(),
            presentation_order: Vec::new(),
            sorted_cts: Vec::new(),
            cts_to_decode: Vec::new(),
        }
    }

    pub fn build_indexes(&mut self) {
        self.build_dts_index();
        self.build_chunk_index();
        self.build_ctts_prefix_sums();
        self.build_sample_ranges();
    }

    fn build_dts_index(&mut self) {
        self.dts_index.clear();
        if self.stts_entries.is_empty() {
            return;
        }

        let mut sample: u32 = 0;
        let mut dts: u64 = 0;
        let mut entry_idx: usize = 0;
        let mut consumed_in_entry: u32 = 0;

        self.dts_index.push(DtsIndexEntry {
            sample_index: 0,
            cumulative_dts: 0,
            stts_entry_idx: 0,
            samples_in_entry: 0,
        });

        while entry_idx < self.stts_entries.len() {
            let (count, delta) = self.stts_entries[entry_idx];
            let remaining_in_entry = count - consumed_in_entry;

            let next_index_at = ((sample / INDEX_INTERVAL) + 1) * INDEX_INTERVAL;
            let samples_to_next = next_index_at - sample;

            if samples_to_next <= remaining_in_entry {
                dts += samples_to_next as u64 * delta as u64;
                consumed_in_entry += samples_to_next;
                sample += samples_to_next;

                if consumed_in_entry >= count {
                    entry_idx += 1;
                    consumed_in_entry = 0;
                }

                self.dts_index.push(DtsIndexEntry {
                    sample_index: sample,
                    cumulative_dts: dts,
                    stts_entry_idx: entry_idx,
                    samples_in_entry: consumed_in_entry,
                });
            } else {
                dts += remaining_in_entry as u64 * delta as u64;
                sample += remaining_in_entry;
                entry_idx += 1;
                consumed_in_entry = 0;
            }
        }
    }

    fn build_chunk_index(&mut self) {
        self.chunk_index.clear();
        if self.stsc_entries.is_empty() || self.chunk_offsets.is_empty() {
            return;
        }

        let total_chunks = self.chunk_offsets.len() as u32;

        self.chunk_index.push(ChunkIndexEntry {
            sample_index: 0,
            chunk_idx: 0,
            stsc_entry_idx: 0,
            sample_in_chunk: 0,
        });

        let mut sample: u32 = 0;
        let mut stsc_idx: usize = 0;

        for chunk in 0..total_chunks {
            while stsc_idx + 1 < self.stsc_entries.len()
                && chunk + 1 >= self.stsc_entries[stsc_idx + 1].0
            {
                stsc_idx += 1;
            }

            let samples_in_chunk = self.stsc_entries[stsc_idx].1;
            let chunk_end = sample + samples_in_chunk;

            let mut next_index_at = ((sample / INDEX_INTERVAL) + 1) * INDEX_INTERVAL;
            while next_index_at < chunk_end {
                self.chunk_index.push(ChunkIndexEntry {
                    sample_index: next_index_at,
                    chunk_idx: chunk,
                    stsc_entry_idx: stsc_idx,
                    sample_in_chunk: next_index_at - sample,
                });
                next_index_at += INDEX_INTERVAL;
            }

            sample = chunk_end;
        }
    }

    fn build_ctts_prefix_sums(&mut self) {
        let mut sum: u32 = 0;
        self.ctts_prefix_sums = self
            .ctts_entries
            .iter()
            .map(|&(count, _)| {
                sum = sum.saturating_add(count);
                sum
            })
            .collect();
    }

    fn ctts_at(&self, sample: u32) -> (usize, u32) {
        let idx = self.ctts_prefix_sums.partition_point(|&s| s <= sample);
        if idx >= self.ctts_entries.len() {
            return (self.ctts_entries.len(), 0);
        }
        let entry_start = if idx > 0 {
            self.ctts_prefix_sums[idx - 1]
        } else {
            0
        };
        (idx, sample - entry_start)
    }

    fn build_sample_ranges(&mut self) {
        self.sample_ranges.clear();
        if self.sample_count == 0 || self.stts_entries.is_empty() {
            return;
        }

        let n = self.sample_count as usize;
        let mut pts_data: Vec<(i64, u32, u32)> = Vec::with_capacity(n);

        let mut dts: u64 = 0;
        let mut stts_entry_idx: usize = 0;
        let mut stts_consumed: u32 = 0;
        let mut ctts_entry_idx: usize = 0;
        let mut ctts_consumed: u32 = 0;

        for sample_idx in 0..self.sample_count {
            let duration = if stts_entry_idx < self.stts_entries.len() {
                self.stts_entries[stts_entry_idx].1
            } else {
                0
            };

            let comp_offset = if ctts_entry_idx < self.ctts_entries.len() {
                self.ctts_entries[ctts_entry_idx].1
            } else {
                0
            };

            let cts = dts as i64 + comp_offset;
            pts_data.push((cts, duration, sample_idx));

            if stts_entry_idx < self.stts_entries.len() {
                dts += duration as u64;
                stts_consumed += 1;
                if stts_consumed >= self.stts_entries[stts_entry_idx].0 {
                    stts_entry_idx += 1;
                    stts_consumed = 0;
                }
            }

            if !self.ctts_entries.is_empty() && ctts_entry_idx < self.ctts_entries.len() {
                ctts_consumed += 1;
                if ctts_consumed >= self.ctts_entries[ctts_entry_idx].0 {
                    ctts_entry_idx += 1;
                    ctts_consumed = 0;
                }
            }
        }

        pts_data.sort_by_key(|&(cts, _, _)| cts);

        self.presentation_order = vec![0u32; n];
        self.sorted_cts = Vec::with_capacity(n);
        self.cts_to_decode = Vec::with_capacity(n);
        for (rank, &(cts, _, sample_idx)) in pts_data.iter().enumerate() {
            self.presentation_order[sample_idx as usize] = rank as u32;
            self.sorted_cts.push(cts);
            self.cts_to_decode.push(sample_idx);
        }

        let (first_cts, first_dur, first_idx) = pts_data[0];
        let mut range_first_sample = first_idx;
        let mut range_last_sample = first_idx;
        let mut range_start = first_cts;
        let mut range_end = first_cts + first_dur as i64;

        for &(cts, dur, sample_idx) in &pts_data[1..] {
            let sample_end = cts + dur as i64;
            if cts == range_end {
                range_last_sample = sample_idx;
                range_end = sample_end;
            } else {
                self.sample_ranges.push(SampleRange {
                    first_sample: range_first_sample,
                    last_sample: range_last_sample,
                    start_time: range_start,
                    end_time: range_end,
                });
                range_first_sample = sample_idx;
                range_last_sample = sample_idx;
                range_start = cts;
                range_end = sample_end;
            }
        }

        self.sample_ranges.push(SampleRange {
            first_sample: range_first_sample,
            last_sample: range_last_sample,
            start_time: range_start,
            end_time: range_end,
        });

        self.sample_ranges.sort_by_key(|r| r.start_time);
    }

    pub fn get_samples(&self, start: u32, count: u32) -> Vec<SampleInfo> {
        if start >= self.sample_count {
            return Vec::new();
        }
        let end = (start + count).min(self.sample_count);
        let mut result = Vec::with_capacity((end - start) as usize);

        let (mut dts, mut stts_entry_idx, mut stts_consumed) = self.dts_at(start);
        let (mut chunk_idx, mut stsc_entry_idx, mut sample_in_chunk, mut chunk_start_sample) =
            self.chunk_at(start);
        let (mut ctts_entry_idx, mut ctts_consumed) = if self.ctts_entries.is_empty() {
            (0, 0)
        } else {
            self.ctts_at(start)
        };

        for sample_idx in start..end {
            let size = self.sample_size(sample_idx);
            let offset = self.sample_offset(
                chunk_idx,
                sample_in_chunk,
                &self.chunk_offsets,
                chunk_start_sample,
            );
            let (is_sync, stss_entry_index) = self.sync_index(sample_idx);
            let composition_offset = if ctts_entry_idx < self.ctts_entries.len() {
                self.ctts_entries[ctts_entry_idx].1
            } else {
                0
            };

            let duration = if stts_entry_idx < self.stts_entries.len() {
                self.stts_entries[stts_entry_idx].1
            } else {
                0
            };

            let description_index = self
                .stsc_entries
                .get(stsc_entry_idx)
                .map(|e| e.2)
                .unwrap_or(1);

            let ctts_entry_index = if !self.ctts_entries.is_empty()
                && ctts_entry_idx < self.ctts_entries.len()
            {
                Some(ctts_entry_idx as u32)
            } else {
                None
            };

            let presentation_index = self
                .presentation_order
                .get(sample_idx as usize)
                .copied()
                .unwrap_or(sample_idx);

            result.push(SampleInfo {
                index: sample_idx,
                presentation_index,
                offset,
                size,
                decode_timestamp: dts,
                duration,
                composition_offset,
                is_sync,
                chunk_index: chunk_idx,
                description_index,
                stts_entry_index: stts_entry_idx as u32,
                ctts_entry_index,
                stsc_entry_index: stsc_entry_idx as u32,
                stss_entry_index,
                uses_co64: self.uses_co64,
                has_variable_sizes: self.constant_sample_size.is_none(),
                sdtp: self
                    .sdtp_flags
                    .get(sample_idx as usize)
                    .map(|&(a, b, c, d)| SdtpFlags {
                        is_leading: a,
                        depends_on: b,
                        depended_on: c,
                        has_redundancy: d,
                    }),
            });

            if stts_entry_idx < self.stts_entries.len() {
                let (count, delta) = self.stts_entries[stts_entry_idx];
                dts += delta as u64;
                stts_consumed += 1;
                if stts_consumed >= count {
                    stts_entry_idx += 1;
                    stts_consumed = 0;
                }
            }

            if !self.ctts_entries.is_empty() && ctts_entry_idx < self.ctts_entries.len() {
                ctts_consumed += 1;
                if ctts_consumed >= self.ctts_entries[ctts_entry_idx].0 {
                    ctts_entry_idx += 1;
                    ctts_consumed = 0;
                }
            }

            sample_in_chunk += 1;
            let samples_per_chunk = self
                .stsc_entries
                .get(stsc_entry_idx)
                .map(|e| e.1)
                .unwrap_or(1);
            if sample_in_chunk >= samples_per_chunk {
                chunk_idx += 1;
                sample_in_chunk = 0;
                chunk_start_sample = sample_idx + 1;

                if stsc_entry_idx + 1 < self.stsc_entries.len()
                    && chunk_idx + 1 >= self.stsc_entries[stsc_entry_idx + 1].0
                {
                    stsc_entry_idx += 1;
                }
            }
        }

        result
    }

    fn dts_at(&self, sample: u32) -> (u64, usize, u32) {
        let idx = self.dts_index.partition_point(|e| e.sample_index <= sample);
        let entry = if idx > 0 {
            &self.dts_index[idx - 1]
        } else if !self.dts_index.is_empty() {
            &self.dts_index[0]
        } else {
            return (0, 0, 0);
        };

        let mut dts = entry.cumulative_dts;
        let mut entry_idx = entry.stts_entry_idx;
        let mut consumed = entry.samples_in_entry;
        let mut current = entry.sample_index;

        while current < sample && entry_idx < self.stts_entries.len() {
            let (count, delta) = self.stts_entries[entry_idx];
            let remaining = count - consumed;
            let needed = sample - current;

            if needed <= remaining {
                dts += needed as u64 * delta as u64;
                consumed += needed;
                current = sample;
                if consumed >= count {
                    entry_idx += 1;
                    consumed = 0;
                }
            } else {
                dts += remaining as u64 * delta as u64;
                current += remaining;
                entry_idx += 1;
                consumed = 0;
            }
        }

        (dts, entry_idx, consumed)
    }

    fn chunk_at(&self, sample: u32) -> (u32, usize, u32, u32) {
        let idx = self.chunk_index.partition_point(|e| e.sample_index <= sample);
        let entry = if idx > 0 {
            &self.chunk_index[idx - 1]
        } else if !self.chunk_index.is_empty() {
            &self.chunk_index[0]
        } else {
            return (0, 0, 0, 0);
        };

        let mut chunk_idx = entry.chunk_idx;
        let mut stsc_entry_idx = entry.stsc_entry_idx;
        let chunk_first_sample = entry.sample_index - entry.sample_in_chunk;
        let mut current_sample = entry.sample_index;
        let total_chunks = self.chunk_offsets.len() as u32;

        let samples_per_chunk = self.stsc_entries[stsc_entry_idx].1;
        let remaining_in_chunk = samples_per_chunk - entry.sample_in_chunk;
        if current_sample + remaining_in_chunk > sample {
            let sample_in_chunk = entry.sample_in_chunk + (sample - current_sample);
            return (chunk_idx, stsc_entry_idx, sample_in_chunk, chunk_first_sample);
        }
        current_sample += remaining_in_chunk;
        chunk_idx += 1;

        while chunk_idx < total_chunks && current_sample <= sample {
            while stsc_entry_idx + 1 < self.stsc_entries.len()
                && chunk_idx + 1 >= self.stsc_entries[stsc_entry_idx + 1].0
            {
                stsc_entry_idx += 1;
            }

            let samples_per_chunk = self.stsc_entries[stsc_entry_idx].1;

            if current_sample + samples_per_chunk > sample {
                let sample_in_chunk = sample - current_sample;
                return (chunk_idx, stsc_entry_idx, sample_in_chunk, current_sample);
            }

            current_sample += samples_per_chunk;
            chunk_idx += 1;
        }

        (chunk_idx, stsc_entry_idx, 0, current_sample)
    }

    fn sample_size(&self, index: u32) -> u32 {
        if let Some(constant) = self.constant_sample_size {
            constant
        } else {
            self.sample_sizes.get(index as usize).copied().unwrap_or(0)
        }
    }

    fn sample_offset(
        &self,
        chunk_idx: u32,
        sample_in_chunk: u32,
        chunk_offsets: &[u64],
        chunk_start_sample: u32,
    ) -> u64 {
        let base = chunk_offsets.get(chunk_idx as usize).copied().unwrap_or(0);
        if sample_in_chunk == 0 {
            return base;
        }

        let mut offset = base;
        for i in 0..sample_in_chunk {
            let si = (chunk_start_sample + i) as usize;
            offset += self.sample_size(si as u32) as u64;
        }
        offset
    }

    fn sync_index(&self, sample: u32) -> (bool, Option<u32>) {
        match &self.sync_samples {
            None => (true, None),
            Some(syncs) => match syncs.binary_search(&(sample + 1)) {
                Ok(idx) => (true, Some(idx as u32)),
                Err(_) => (false, None),
            },
        }
    }

    pub fn get_edited_ranges(
        &self,
        movie_timescale: u32,
        media_timescale: u32,
    ) -> Vec<EditedRange> {
        if self.edit_list.is_empty() {
            return Vec::new();
        }

        let movie_ts = movie_timescale as f64;
        let media_ts = media_timescale as f64;

        let mut ranges = Vec::with_capacity(self.edit_list.len());
        let mut presentation_offset: i64 = 0;

        for (i, entry) in self.edit_list.iter().enumerate() {
            let seg_dur = entry.segment_duration as i64;
            let is_empty = entry.media_time == -1;

            if is_empty {
                ranges.push(EditedRange {
                    presentation_start: presentation_offset,
                    presentation_end: presentation_offset + seg_dur,
                    media_start: -1,
                    media_end: -1,
                    edit_index: i as u32,
                    is_empty: true,
                    media_rate: entry.media_rate,
                });
            } else {
                let rate = if entry.media_rate == 0.0 {
                    1.0
                } else {
                    entry.media_rate
                };
                let consumed = (seg_dur as f64 / movie_ts) * rate * media_ts;
                let media_end = entry.media_time + consumed as i64;

                ranges.push(EditedRange {
                    presentation_start: presentation_offset,
                    presentation_end: presentation_offset + seg_dur,
                    media_start: entry.media_time,
                    media_end,
                    edit_index: i as u32,
                    is_empty: false,
                    media_rate: rate,
                });
            }

            presentation_offset += seg_dur;
        }

        ranges
    }

    pub fn find_sample_at_cts(&self, target_cts: i64) -> u32 {
        if self.sorted_cts.is_empty() {
            return self.sample_count;
        }
        let rank = self.sorted_cts.partition_point(|&cts| cts < target_cts);
        if rank >= self.sorted_cts.len() {
            self.sample_count
        } else {
            self.cts_to_decode[rank]
        }
    }

    pub fn find_sample_at_time(&self, target_dts: u64) -> u32 {
        if self.dts_index.is_empty() || self.sample_count == 0 {
            return self.sample_count;
        }

        let idx = self
            .dts_index
            .partition_point(|e| e.cumulative_dts <= target_dts);
        let entry = if idx > 0 {
            &self.dts_index[idx - 1]
        } else {
            &self.dts_index[0]
        };

        let mut dts = entry.cumulative_dts;
        let mut entry_idx = entry.stts_entry_idx;
        let mut consumed = entry.samples_in_entry;
        let mut current = entry.sample_index;

        if dts >= target_dts {
            return current;
        }

        while current < self.sample_count && entry_idx < self.stts_entries.len() {
            let (count, delta) = self.stts_entries[entry_idx];
            let remaining = count - consumed;

            if delta == 0 {
                current += remaining;
                entry_idx += 1;
                consumed = 0;
                continue;
            }

            let dts_gap = target_dts - dts;
            let samples_needed = ((dts_gap + delta as u64 - 1) / delta as u64) as u32;

            if samples_needed <= remaining {
                current += samples_needed;
                return current.min(self.sample_count);
            }

            dts += remaining as u64 * delta as u64;
            current += remaining;
            entry_idx += 1;
            consumed = 0;

            if dts >= target_dts {
                return current.min(self.sample_count);
            }
        }

        self.sample_count
    }
}
