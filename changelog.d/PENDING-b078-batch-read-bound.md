### Fixed

- Bound native CAS `batch-read` to the budget-derived eight-task window: one 22 MiB
  envelope plus eight 24 MiB object reservations peaks at 214 MiB within the
  220 MiB read slice. Pass the 8 MiB per-object ceiling to storage before body
  collection, return `413 batch_too_large` for over-cap objects or aggregate
  responses, and drain pending tasks before terminal errors return.
