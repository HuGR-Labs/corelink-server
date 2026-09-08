# B-321: keep alert delivery tests strict-lint clean

The alert and recovery recording-transport tests now assert their fallible
results with actionable diagnostics instead of unwrapping them. Their delivery
count, channel-order and recovery-receipt assertions remain unchanged.
