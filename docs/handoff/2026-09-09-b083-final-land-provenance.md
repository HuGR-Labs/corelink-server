# B-083 final LAND composition provenance

Complete B-083 NET replay and repair composition on the exact shared sprint head. The 87 source commits were replayed in topological order; this is not a tip-only cherry-pick.

- shared sprint base: `9d19f12ffd3aac17419ca6e32969ff8f2a9a70b3`
- B083 source base: `0227d446d251137fd8abc7507161735873d80152`
- B083 source tip: `6b818d3ae8e50dca4e8add3d9b0ea4d178fe325e`
- generated B083 replay tip: `3171cf1ac`
- source/replay count: `87/87`
- DCO: all 87 source commits and all generated replay commits carry `Signed-off-by:`; this provenance commit is also DCO-signed.
- semantic conflict: `crates/corelink-container/src/storage/d1_http.rs` retained the sprint read-only/select guard and integrated B083 contradictory-error fail-closed validation. The test-only compatibility helper remains behind `#[cfg(test)]`.
- semantic conflict: `crates/corelink-container/src/storage/byok_activation_d1.rs` retained timeout-abort tests and added the zero-source retirement fixture/tests in a separate module.
- dependency overlap: duplicate B083 `blake3` declaration was removed; the sprint's canonical dependency declaration is retained.
- migrations: `0118`–`0121`, existing `0122`–`0124`, then repair migrations `0125` and `0126`; all additive and uniquely prefixed.
- production safety: `byok_backfill_d1` remains `#[cfg(test)]` only.

## Serial repair stacks

| Source repair | Generated commit |
|---|---|
| `dbf56ff94e3a8b46a733579a4018c087d880a357` | `c32f7725c` |
| `b30d864a4` | `de7801cd6` |
| `98de1f6ec255611219430f2a7e9ab5a4048a6f89` → `ae4d1f935` | `ace34ccc0` → `b7f4362b6` |
| `7ac443f68278cb30b2ba85dda136ec394bc3726e` → `291933a74` | `93f4c4e05` → `7912a4eb8` |
| `7fcf63924` → `5bbe70a68` → `9311c959d` → `d6c9c6034` → `ca798ce8f` | `e8a17d93c` → `1c6f41c98` → `67a419db8` → `089c2aa61` → `1a8c9a958` |
| composition repair: duplicate dependency removal | `a8c13921a` |

## 87-commit replay map

| # | Source commit | Generated replay commit | Subject | Resolution |
|---:|---|---|---|---|
| 1 | `e58f20a0a672936f57f95012b9f6a02b3b5f76e8` | `c615c5bc4fccf8541e01a2c90203e163d5cfaef2` | fix(B-083): wire real KMS into CAS and AC | replayed |
| 2 | `8de258a3d00eef2a1947259f0db93dade5a8973e` | `8b719c763446d7012a5326b9039295cf4851d4fe` | feat(B-083): add tenant-wide BYOK transition fencing | replayed |
| 3 | `a7fb424883127b2cf5c39cf9191d7a8ba337489f` | `a850afcfa9370adf94775a97c4343cebb41f06e2` | fix(B-083): harden BYOK fence lifecycle | manual d1_http semantic merge |
| 4 | `35e2ea6234b1f7b6c4867e5e21dca740054a0b86` | `6bb413e9244d2388a0fa51d1cb09f521d61af6b1` | fix(B-083): fence BYOK control transitions per tenant | replayed |
| 5 | `9ba00c4f834e2acbef86a3555e04cc55e27431b1` | `bcdf9e91c448097f6e54c1a9ac8c391dbdcd2b7d` | feat(B-083): add resumable BYOK backfill engine | replayed |
| 6 | `2bd3c5a7fde1dabfea9e54c8e18b955349c746d3` | `c5d38e02a9fc7e739485e69ca934f9404ffbca90` | feat(B-083): bind backfill ledger to generation fencing | replayed |
| 7 | `65ab22a487fab3c302a298b047b21f2fe341d87a` | `516eed68fd1fb3afaf8ec4c4dc75a08737a25d0f` | fix(B-083): make backfill takeover crash-safe | replayed |
| 8 | `2093f5a366d62638cf444ceb9ac7268449b4c9b9` | `ea7632f06115f3d1809d0a083cb20daa6ff5d38a` | test(B-083): pin fenced revocation control wiring | replayed |
| 9 | `b98164ea7e5a8e0d4584d637b785839e1024e3a4` | `cd57289e06a3a1076745c99b1d69a24fc2ddc975` | fix(B-083): repair focused backfill compilation | replayed |
| 10 | `bf4a025a579944a295ee50d8e96bb7eedc0e35d1` | `bac76fb52168b8253fda612347ad8987489a725d` | fix(B-083): zeroize backfill plaintext buffers | replayed |
| 11 | `38840db897b2a40208cfc7c9f618a3b36f9fae96` | `19cba318f12b1d0663fefe287a880b51dcf8c5d8` | feat(B-083): fence CAS and AC through generation catalog | replayed |
| 12 | `5516e92f7717075aee4c454a267aba9e52be1fb5` | `43ae32716722fbae9d6bb29d5a6c3877385c851b` | chore(B-083): normalize storage module export | replayed |
| 13 | `dc6d5e1df600a0336f0af2b0845fae3940224e39` | `72d8e2a75b56492067778b731bd4346144882f99` | fix(B-083): checkpoint opaque backfill page cursors | replayed |
| 14 | `299db2d5dab5c614e09e1151b968b2c8ea46c2cb` | `690377e6efa51ef1a4e78276af8dc73d08fbb189` | feat(B-083): add production BYOK backfill adapter | replayed |
| 15 | `5cdde12e747c5f6bca760920e35f441bf1dd8670` | `c27d9fabd933e3b2675351077a34a89987cef712` | fix(B-083): document fenced control API | replayed |
| 16 | `95cc00f54c3aa566d2d45d90bf02a1db449bd791` | `74ddb4609da24137b14e1d3c13291d536ca1ad1e` | fix(B-083): align committed fence outcome | d1_http merge propagation |
| 17 | `61564c58d97748abfa66772de7b764af070a56ce` | `08964f4d73de20ae4236d652553e0cd34910429a` | fix(B-083): qualify Mode B envelopes by allocation | replayed |
| 18 | `3b1710996ecd7e6e7a5c8c11f7246d9d8b90f8c1` | `cb15228fab364491cfb9288fcf94e6c9f3ec2c52` | feat(B-083): harden allocation-safe backfill crypto | replayed |
| 19 | `16223f24158827e88543ac5845a86a16808c76a2` | `b07eb40f5a8e301a129d1ac8e3a481f954a6c9ba` | fix(B-083): document backfill public API | replayed |
| 20 | `c05636e313204b1ab25c355f1830d80ea31f11e3` | `0180a945d88e7a3036d8c09c25bb834cfb6145cb` | fix(B-083): bind catalog size and opaque CAS cursor | replayed |
| 21 | `5c26ac92cfe9e8f1418b507e98d4a1c0283bcb06` | `ec35b1f4b1a009cd505fb4b28a595cf3769bc76a` | feat(B-083): define purge-before-active pipeline | replayed |
| 22 | `4b1ba0fa7cb3ee29caddd63b2102956a117f7ff3` | `a1a57e06f2d30a7c01554b8fe99b43659110a428` | style(B-083): normalize activation module export | replayed |
| 23 | `01f8313bd84274276325c7a829126646fd7b9c5c` | `5f287cfffe72104ff2dbf9f56a6f9819fe4fe54a` | fix(B-083): complete activation schema capability | replayed |
| 24 | `148894f326a544f97996249c37b83d8cee64b603` | `3478de810786410221934ed25818d90c43b0eef2` | feat(B-083): decode rotated source generations | replayed |
| 25 | `406bcb34cd352d2b356bba209ebba4f38ab2414b` | `e2abd1e330a2b1a585e46421cc8351c4d335daad` | fix(B-083): bind source history and provider identity | replayed |
| 26 | `56c9e16a54b31e802eed942e58c3994a7e38e4b4` | `c5240e5346bcde36130f9bda59a2ba151c33cc5a` | fix(B-083): close legacy source identity gaps | replayed |
| 27 | `987365873bc5444d9b521e4a9118563ccd85f4a8` | `9737c7b814fc8772a78fbca39200549409e5f233` | feat(B-083): add guarded activation lifecycle adapter | replayed |
| 28 | `992474f2ab929bbaaf0fcbfe292703a188d5de75` | `56b7742bbc138ad53a1a8d27e764ebf4ce9f0ea0` | fix(B-083): preserve activation fence exclusion | replayed |
| 29 | `fbae505b4976d1e003017873013421a32cdc7a1d` | `fb2c4a43485e97e3da951ff9f109f6801acc8ac3` | fix(B-083): restore exact config history reader | replayed |
| 30 | `c314041bd50b3addf9f4849e3f8e0e0ff5afe7aa` | `58a4e86d2bfadd99581b68af8df913aaa79f7a2e` | fix(B-083): preserve exact rotation cleanup identity | replayed |
| 31 | `8b7029a6c6097005bbd1b25b30dfe1493d81540d` | `2765daf5de18a61eccd65cb0486c0b5a6aadc9d9` | fix(B-083): prove every activation purge cause | replayed |
| 32 | `3f15c2524857af75e6dce16cfb672f31181b3594` | `b76e500724c3cb2c88c2bd3c40eb565292efe936` | fix(B-083): fence source and target activation ledgers | replayed |
| 33 | `91fb892b0df643ce0c4d58bcd7722bb40f2f838e` | `949234b62bf55918605bbd44356b4ed54372f8bd` | feat(byok): persist activation intent atomically | replayed |
| 34 | `4e018465ec28d2c18a7e7be496354a3776bec652` | `1e5ada82ef34a61ed1113f2199792e45f76bac67` | fix(B-083): authorize activation source discovery | replayed |
| 35 | `5297b86df0a286451a1f479cf049f091660cfc7e` | `6a0ba3a734011461630490f8dbe75ebc4849cb51` | fix(byok): preempt activation on destructive deactivate | replayed |
| 36 | `fc5a07ce7f67854a3a589d5a6d492cc361dcc49d` | `9b30850670f3cd6b87c017c7024f4e33a71257d8` | fix(B-083): reserve activation ciphertext before IO | replayed |
| 37 | `98c52b32effc7b154a3529504819ae92bd5600f6` | `302ad8448afcb70f4cdf1af59412462eeea3c9e4` | fix(B-083): enforce forward-only activation generations | replayed |
| 38 | `ee39e21e4aaa774041801d6e9b4ca4bb15f3e0ff` | `23d9692cad756bbbafb5d286d09079e873aeb69c` | fix(byok): isolate destructive control fence | replayed |
| 39 | `c4dc7b85cd9c68c63074fc97dc9a529bdd1d7324` | `772d8f0c61462986eca450e7840266cd87294656` | fix(B-083): gate finalize on purge causes | replayed |
| 40 | `0fb54bd7dcff703e8d64e88b82d80d60754d1efa` | `a0543369ca9467860024fdeb40500f72e4999528` | fix(byok): cancel unpublished activation truthfully | replayed |
| 41 | `cd4334008a1128374228c74407233465bfbeb2c9` | `49b816da2092390da2156b0abd21a7d644fd1283` | feat(B-083): supervise bounded activation jobs | replayed |
| 42 | `9f2bdebca78438c343d36001da4aeaee5b67ec0c` | `f534ae884307ce339e6e7f0b2bb98e6abb346ad1` | fix(byok): bypass KMS for durable activation retries | replayed |
| 43 | `6767995feecd853189027ce25c353313f1d32dc2` | `b6dc29fd127006e65924aa3df4e58199566fd149` | test(byok): prove cancellation safety barriers | replayed |
| 44 | `93cd5d3f498d45f34aff17f014b95509c8c89b59` | `bab5d20a6ddacdd0132e817fdf908126193e8203` | fix(byok): suspend live activation on CMK outage | replayed |
| 45 | `d071d76d740e063392baf63574a7554e2f5bf142` | `84737c899d7367f55ba6366964dbf0b6489a94d4` | test(byok): pin live activation suspension protocol | replayed |
| 46 | `3aed7e1ec061e9f21c565965b2463090ae45813c` | `9b09a91b0fd4e0fd3110a943e695c3d0be308f15` | fix(byok): monitor rotation source and target keys | replayed |
| 47 | `1603a2ed3b8a9e2950bd3ceed07e975af78137ba` | `3469a1e620b84350af35dd6bf23240f6c0cf311f` | fix(byok): pin exact activation source custody | replayed |
| 48 | `2108d4e70715a6e2149ed4bceaa82938183e7923` | `1a2e3cad541be02e003a8ca9a008c6a43a1b3258` | fix(byok): separate activation cancel from shred | replayed |
| 49 | `423fc2339995fe524121e5029cde56048fc1d392` | `b03d33965a5075d28a93eba1a7a7b2a5b833af43` | fix(byok): fence rotation dependency recovery | replayed |
| 50 | `9c7b709269debaf7b7daf53d3ef77ddb8781d6de` | `6f28bd0df93e265fbddccd49fa128bf70b53e7c0` | fix(byok): require explicit deactivate action | replayed |
| 51 | `fe318c921151ac729b6a74b0f99460c0f61373ca` | `af2464a7c594f123cd82f0071ca7ab2d67e30d49` | fix(byok): require storage before KMS preflight | replayed |
| 52 | `7672c0561b39a7ec3c1e53ac9aa7e3b5e2aacc2f` | `5a3b99f5dbed398f9ab2e91cbd6f77d073a260fc` | test(byok): execute legacy rotation authority probe | replayed |
| 53 | `504c444327431e94cea7c007cd9b6f6a3a71df68` | `e99df4cbfbf3ff2e61a35ffc5ddcdf7b87d55686` | fix(byok): authorize pinned source suspension | replayed |
| 54 | `d60691692a6597bad35072f99a5513702c21a082` | `a4f5f691e2fee02da5ebcc7bd4a66fa95251d093` | feat(B-083): implement activation object worker | replayed |
| 55 | `78bce4a663a2051434236769eba86a612067e725` | `62d7f7a32edf5319aa70f24cfa29093f5a266d2d` | fix(B-083): close activation object races | replayed |
| 56 | `c6c2a142701aa61c89cff8394f08808d4d83988c` | `8cba2b64d4522b8353403d78be85c56e682c7e78` | test(B-083): assert activation race order | replayed |
| 57 | `12aedbc5091ff41ba3c15f6a9fef658aaf4e4592` | `80245e0c86366abaed2cbe030b19b8ecf2891d99` | feat(B-083): durably purge live-delete and publish losers | replayed |
| 58 | `d58e06eb20076b705f1f0b5a0197280d49529d4e` | `9bc33ae97409c36dd0185295682fee37f7f7221c` | fix(B-083): harden durable purge claims and retries | replayed |
| 59 | `62a299813b11692ea2d4f32033440534f86bc89e` | `b9b8de0fc455f3a1fda0d3f4a8ae92388c59c5f1` | fix(B-083): gate stale census by terminal ownership | replayed |
| 60 | `b9d2f1c03713fe6849d265c0329c5253086e5f74` | `a7f9bd57c064326c93c4e9c69731046b7038e321` | fix(B-083): add periodic purge reconciler seam | replayed |
| 61 | `4e6b95de34f7bdd0037f7f850a83f7534bb5b125` | `a6af4aa2bcb3d506566609f875a2efcb97bb03b4` | feat(B-083): drive exact live purge I/O | replayed |
| 62 | `8dc3dbc000403b4eb83908468a0d756512517d17` | `34421ffb5360fcfee7804fbde3d07ca3f024e449` | fix(B-083): verify purge absence after delete errors | replayed |
| 63 | `8a6e177d1eaae067939db6fb2e1a0e73fc04d530` | `7e9019799f5c3d09545496d3f7bac8f72e1bdfce` | fix(B-083): drain purge owners before deletion | replayed |
| 64 | `01352492b8ebdb15c2a2b7e628529af52e58e22c` | `68813e9fe94609b839b8349ee48c3b0823c945b3` | fix(B-083): fence purge behind write expiry | replayed |
| 65 | `e82b5a1eb9abfd1c69c3cb13d7fb898a381e9f06` | `58f4c32cef1d179f3cc59797e8c7f5ba43f26a48` | fix(B-083): require backfill PUT lease headroom | replayed |
| 66 | `723126e24939298ea7fd5055f3c6f03475b9559a` | `dd33ecf50fe27f6cb604ace055de068daf219e50` | fix(B-083): retire unsafe generic backfill runtime | replayed |
| 67 | `daac46fdf401f9240b6865a4cb6776ceea0f1ba9` | `7492eaf0bf3f89bfa4fb779f1ede31f6912d8258` | fix(B-083): bound stale allocation census | replayed |
| 68 | `49c8fa2be118aaed58d5117950c20c5e1ff53b3b` | `958bb51ea4860579e9bb2af32e0ee3afbd112f50` | fix(B-083): compose purge lease guards | replayed |
| 69 | `d98bd904a5a5d42104902b9930be2f4c924e4423` | `383232ff06a0727a05fe4ebbb3ce4a7d9b3a10c0` | fix(B-083): qualify activation object allocations | replayed |
| 70 | `a413642428b53d92e9e563c0e9a7d94cb16d33e4` | `d73694803cffe4cbb85cf253e291bdbf604e759c` | test(B-083): cover common loser handoff | replayed |
| 71 | `6ad19905ed66ab5713cda2060859c0273aa0cd9f` | `98e541e402404e28fac8e520ec1f0461db0bea54` | fix(byok): recover exact stale purge obligations | replayed |
| 72 | `ab6879a3051d4bc7b3cf9d4e908a0f6f86423d03` | `a7478e5d1104ec0c726b8ada08665d9adbf78ed1` | fix(byok): census stale purge work before claims | replayed |
| 73 | `07813e2fd687b90475124bf08ffa44d608935ede` | `91da419f9253a3a45dd2dafabb89f30e64eeb2b1` | fix(B-083): harden activation storage digests | replayed |
| 74 | `ba6278ea3c9461e7fe0507a59a87f2022bf2f7f6` | `156ad6b79eedc56e5420c6cf861dd6ff41f52823` | feat(B-083): start bounded BYOK background workers | replayed |
| 75 | `a7549110042f05165839f736c7e6246be742471d` | `c8c324ada1f6cfbbc929d6739543c14b4eca3869` | fix(B-083): bound activation below target lease | replayed |
| 76 | `38c63236db301a5ca33fdf1e5e10337cec890323` | `fe80f68dad3fa3a144dfdf6c906fac7a32694ae6` | fix(B-083): key activation loser causes by allocation | replayed |
| 77 | `02da2cfae486b962a93b62c630fca2b279b005b7` | `944f52e3c9ce13eae5d8087bea1e7fe23a355309` | fix(B-083): bind purge validation collaborators | replayed |
| 78 | `05dc3a15ff9f30bd89e83797394a69a2af117fb4` | `2bdd2bb84c83452e120047b4057115dc5a1b2500` | fix(byok): own purge allocation before mutation | replayed |
| 79 | `cf0d9ad2006bce5a3d361d0552483f7fb4e8d6e6` | `d47036fb8103bd07ab87e4477b4c009e7d7dbc7f` | docs(B-083): document activation worker seams | replayed |
| 80 | `e7a1e1a6094b279f001ff211ccdc8e8f43cd3a3f` | `227edfed0617a2e987c604a5bfa13dfff373c656` | fix(byok): remove unused activation snapshot field | replayed |
| 81 | `2845f94ebb292583c598dd726032593b112159eb` | `80aa8d43726a73bea37a2383be1b4615106daeb9` | fix(byok): simplify expired guard binding | replayed |
| 82 | `7d1ad24f68de86741ea03595366130aa6ffbc943` | `63f9a3ba40822c5f4890628994371eb8fb9741be` | style(B-083): satisfy activation worker clippy | replayed |
| 83 | `03a113f0a0ccea23c3ab9928f78da680b5647bc1` | `4d4f0795c00d3ec64850ceb9340eff0401478380` | chore(byok): scope purge lint exceptions | replayed |
| 84 | `111829474d1f013b76ecbf6a1a2175f2db9d88cf` | `e4713fd77aaae699043343025568cbb197445ef3` | style(B-083): finish focused clippy cleanup | replayed |
| 85 | `01c63e815f89b4a3cce60970591ae43b931505a1` | `dc0ee50d8fa488a83fc2b95ebb891cf329f617d1` | test(B-083): align activation test seams | replayed |
| 86 | `5f383b5b019440ca9fa2c256476944d1e292137c` | `c5edef44d95c7a27f455719ec1b2711f162aae7c` | test(B-083): refresh activation fixtures | replayed |
| 87 | `6b818d3ae8e50dca4e8add3d9b0ea4d178fe325e` | `3171cf1acd067e61b6f0a7a70c8b39c24d729279` | fix(byok): reject shredded data intents | replayed |
