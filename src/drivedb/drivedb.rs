use super::{filter_presets, presets, vendor_attribute, Attribute};
use super::parser::Entry;
use regex::bytes::RegexSet;
use std::collections::HashSet;

use ata::data::id;

#[derive(Debug)]
pub struct DriveDB {
	entries: Vec<Entry>,

	// pre-found default entry: most likely it will be used right away, so it's not that harmful,
	// and it's better to have one if it's going to be requested multiple times
	default: Option<Entry>,

	// precompiled RegexSets are often faster than simple regexes lazily compiled one by one on demand until the first match
	// (even if RegexSet compilation time is taken into account!),
	// and are a must if multiple lookups are about to be performed
	model_regexes: RegexSet,
	firmware_regexes: RegexSet,
}

impl DriveDB {
	pub(crate) fn new(entries: Vec<Entry>) -> Self {
		let entries = entries.into_iter()
			// USB ID entries are parsed differently; also, we don't support USB devices yet
			.filter(|e| ! e.model.starts_with("USB:"));

		// filter out all entries marked as default: they're of no use fo self.find()
		// (yes, there might be multiple default entries from e.g. additional drivedb files)
		let (default, entries): (Vec<_>, Vec<_>) = entries.partition(|e| e.family == "DEFAULT");

		// pick the first default entry, if any, or set to None
		let default = default.into_iter().next();

		// model and firmware are expected to be ascii strings, no need to try matching unicode characters
		let model_regexes = RegexSet::new(entries.iter()
			.map(|e| format!("^(?-u){}$", e.model))
		).unwrap();
		let firmware_regexes = RegexSet::new(entries.iter()
			.map(|e|
				if e.firmware.is_empty() {
					"(?-u)".to_string()
				} else {
					format!("^(?-u){}$", e.firmware)
				}
			)
		).unwrap();

		DriveDB {
			entries,
			default,
			model_regexes,
			firmware_regexes,
		}
	}

	pub(crate) fn find(&self, model: &str, firmware: &str) -> Option<&Entry> {
		let models: HashSet<_> = self.model_regexes.matches(model.as_bytes()).iter().collect();
		let firmwares: HashSet<_> = self.firmware_regexes.matches(firmware.as_bytes()).iter().collect();

		// find the first match (if any)
		models.intersection(&firmwares)
			.min()
			.map(|index| &self.entries[*index])
	}

	pub(crate) fn get_default_entry(&self) -> Option<&Entry> {
		self.default.as_ref()
	}

	/**
	Matches given ATA IDENTIFY DEVICE response `id` against drive database `db`.

	Return value is a merge between the default entry and the first match; if multiple entries match the `id`, the first one is used (this is consistent with smartmontools' `lookup_drive` function).
	`extra_attributes` are also appended to the list of presets afterwards.

	This functions skips USB ID entries.
	*/
	pub fn render_meta(&self, id: &id::Id, extra_attributes: &Vec<Attribute>) -> DriveMeta {
		let mut m = DriveMeta {
			family: None,
			warning: None,
			presets: Vec::<Attribute>::new(),
		};

		// TODO show somehow whether default entry was found or not, or ask caller for the default entry
		if let Some(default) = self.get_default_entry() {
			// TODO show somehow whether preset is valid or not
			if let Some(presets) = presets::parse(&default.presets) {
				m.presets.extend(presets);
			}
		}

		if let Some(entry) = self.find(&id.model, &id.firmware) {
			// TODO show somehow whether preset is valid or not
			if let Some(presets) = presets::parse(&entry.presets) {
				m.presets.extend(presets);
			}

			m.family = Some(&entry.family);
			m.warning = if ! entry.warning.is_empty() { Some(&entry.warning) } else { None };
		}

		m.presets.extend(extra_attributes.iter().map(|a| a.clone()));
		m.presets = filter_presets(id, m.presets);
		return m;
	}
}

/// Drive-related data that cannot be queried from the drive itself (model family, attribute presets etc.)
#[derive(Debug)]
pub struct DriveMeta<'a> {
	/// > Informal string about the model family/series of a device.
	pub family: Option<&'a String>,

	/// > A message that may be displayed for matching drives.
	/// > For example, to inform the user that they may need to apply a firmware patch.
	pub warning: Option<&'a String>,

	/// SMART attribute descriptions
	presets: Vec<Attribute>,
}

impl<'a> DriveMeta<'a> {
	pub fn render_attribute(&'a self, id: u8) -> Option<Attribute> {
		vendor_attribute::render(self.presets.to_vec(), id)
	}
}
