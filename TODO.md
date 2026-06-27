# TODO.md — memcrabd: memcached 1:1 in Rust nachbauen

> Ziel: Ein vollständiger, kompatibler memcached-Server in Rust.
> Reihenfolge: erst Grundlagen (Protokoll-Abdeckung), dann Interna
> (Speicher/Eviction), dann Spezialisierung (Meta/Binary, Ops, Security).
> Jede Phase ist einzeln testbar (z. B. mit `telnet`/`nc` oder dem
> offiziellen Test-Suite-Kompatibilitäts-Check).

---

## Phase 0 — Projekt-Setup & Architektur-Grundgerüst

- [ ] 0.2 `tracing`/`log` statt `println!`/`eprintln!` einführen
  - `-v` / `-vv` / `-vvv` Verbose-Level (wie memcached)
- [ ] 0.3 CLI-Argument-Parser (z. B. `clap`) für alle memcached-Flags
  - `-p`/`--port` (TCP), `-U` (UDP), `-s` (Unix Socket), `-l` (Listen-Interface)
  - `-m` (Memory in MB), `-M` (kein Eviction, Error bei OOM), `-I` (max Item-Size)
  - `-c` (max Connections), `-t` (Threads), `-v` (Verbose)
  - `-d` (Daemonize), `-u` (User), `-r` (Core-Dump), `-k` (Lock-All-Memory)
  - `-C` (CAS disable), `-A`/`--enable-shutdown`
  - `-o` Extended-Options (slab_reassign, lru_maintainer, modern, track_sizes, …)
- [ ] 0.4 Graceful Shutdown (SIGINT/SIGTERM) via `tokio::signal`
- [ ] 0.5 Konfigurierbare Bind-Adresse (`-l`), Port `0` = zufällig

**Done-Kriterium:** Server startet mit `cargo run -- -p 11211 -m 64 -vv`, loggt
strukturiert, reagiert auf bestehende `set`/`get`/`delete`/`version`.

---

## Phase 1 — Vollständiges Text-Protokoll (basic commands)

Die bestehenden `set`/`get`/`delete`/`version`/`quit` werden ergänzt um alle
fehlenden Basic-Commands. Reihenfolge nach Wichtigkeit.

### 1.1 Storage Commands

- [ ] `add <key> <flags> <exptime> <bytes> [noreply]` — nur speichern wenn Key _nicht_ existiert; Antwort `STORED`/`NOT_STORED`; bei Fehlschlag trotzdem LRU-Bump des existierenden Items
- [ ] `replace <key> <flags> <exptime> <bytes> [noreply]` — nur speichern wenn Key \*existiert`; `STORED`/`NOT_STORED`
- [ ] `append <key> <flags> <exptime> <bytes> [noreply]` — Daten an existierenden Wert anhängen; _ignoriert_ flags/exptime (laut Spec!); prüft `item_size_max` (`-I`)
- [ ] `prepend <key> <flags> <exptime> <bytes> [noreply]` — wie append, aber vorne
- [ ] `cas <key> <flags> <exptime> <bytes> <cas_unique> [noreply]` — `STORED`/`EXISTS`/`NOT_FOUND`
- [ ] Fehler-Strings gemäß Spec:
  - `ERROR\r\n` — unbekannter Befehl
  - `CLIENT_ERROR <msg>\r\n`
  - `SERVER_ERROR <msg>\r\n` (schließt ggf. Verbindung)

### 1.2 Retrieval Commands

- [ ] `gets <key>*` — wie `get`, aber Antwortzeile `VALUE <key> <flags> <bytes> <cas_unique>\r\n`
- [ ] `gat <exptime> <key>*` — Get + Touch (TTL updaten)
- [ ] `gats <exptime> <key>*` — Get + Touch + CAS

### 1.3 Modify Commands

- [ ] `touch <key> <exptime> [noreply]` — `TOUCHED`/`NOT_FOUND`
- [ ] `incr <key> <value> [noreply]` — Wert als u64 interpretieren; `NOT_FOUND`/`<new_value>`; Overflow wrap-around
- [ ] `decr <key> <value> [noreply]` — wie incr, aber Unterlauf -> 0 (kein Wrap)
- [ ] incr/decr müssen fehlschlagen wenn Wert nicht numerisch (`CLIENT_ERROR cannot increment or decrement non-numeric value`)
- [ ] incr/decr erstellen _nicht_ implizit einen Key (Miss = `NOT_FOUND`)

### 1.4 Admin / Maintenance Commands

- [ ] `flush_all [<delay>] [noreply]` — invalidiert alle Items (setzt globalen Flush-Zeitpunkt); `OK`; Items werden lazy beim nächsten Zugriff entfernt
- [ ] `verbosity <level> [noreply]` — setzt Log-Level zur Laufzeit; `OK`
- [ ] `cache_memlimit <megabytes> [noreply]` — Memory-Limit zur Laufzeit anpassen; `OK`
- [ ] `shutdown [graceful]` — Server stoppen (SIGINT/SIGUSR1); nur wenn `-A`
- [ ] `lru_crawler ...` — siehe Phase 4 (später)

### 1.5 Sonstiges

- [ ] Key-Längen-Validierung (max 250 Bytes, keine Control-Chars/Whitespace im ASCII-Modus)
- [ ] `exptime`-Semantik: <= 30 Tage = Offset in Sekunden; > 30 Tage = Unix-Timestamp
- [ ] Negatives `exptime` -> sofort expired (wird bei get nicht gefunden)
- [ ] Korrekte `noreply`-Semantik: Fehler werden _immer_ gesendet (auch bei noreply)

**Done-Kriterium:** Alle Basic-Text-Befehle funktionieren und verhalten sich
byte-genau wie memcached (abgleichbar via `nc`-Skript). Keine `ERROR`/`CLIENT_ERROR`
mehr für `add`/`replace`/`append`/`prepend`/`cas`/`gets`/`gat`/`gats`/`touch`/`incr`/`decr`/`flush_all`.

---

## Phase 2 — Speicher-Internas: Slab Allocator & LRU

Das Herz von memcached. Aktuell nur `HashMap` — das muss durch ein
Slab-basiertes Modell ersetzt werden, sonst ist es kein memcached.

### 2.1 Slab Allocator

- [ ] `SlabClass` mit `chunk_size` (growth factor `1.25` default, `-f`)
  - Default chunk_size start = 48 Bytes (`-n`), wächst bis `slab_chunk_max` (default 1MB / `-I`)
  - `chunks_per_page = page_size / chunk_size` (page = 1MB)
- [ ] `SlabAllocator` verwaltet alle SlabClasses
  - `slabs_clsid(key, value_len)` — passende Klasse finden
  - `slabs_alloc(clsid)` — Chunk aus freelist oder neue Page
  - `slabs_free(ptr, clsid)` — Chunk zurück auf freelist
  - Page-Tracking: `total_pages`, `total_chunks`, `used_chunks`, `free_chunks`
- [ ] Globales Memory-Limit (`-m` in MB); Memory wird beim Start nicht voll
      allokiert (lazy per `mmap`/`malloc`-ähnlich)
- [ ] `item_size_max` (`-I`, default 1MB) — größtes zulässiges Item; bei Überschreitung `SERVER_ERROR object too large for cache` bzw. Text `CLIENT_ERROR bad command line format` + `store_too_large` stat
- [ ] `-M` Modus: kein Eviction, bei OOM `NOT_STORED`/`SERVER_ERROR out of memory`

### 2.2 Hash Table (assoc)

- [ ] Eigene Hash-Tabelle (wie memcached: `assoc.c`)
  - Initiale Größe via `hashpower_init` (`-o hashpower=N`, default 16 = 65536 Buckets)
  - Dynamic expand: wenn Load-Factor überschritten, Hintergrund-Expansion
  - `hash_bytes` Stat
- [ ] Items nicht in `HashMap<String, Item>` sondern über Slab-Chunk-Pointer
      referenziert (`*mut Item` / `NonZero`-basiert)
- [ ] `stats hash` — Hash-Table-Stats

### 2.3 LRU & Eviction

- [ ] Per-Slab-Class LRU (doppelt verkettete Liste)
  - `item_link` / `item_unlink` — Item in LRU ein-/aushängen
- [ ] Eviction bei `slabs_alloc`-Mißerfolg:
  - Tail der LRU der Zielklasse suchen
  - Wenn expired -> `reclaimed`-Stat++, wiederverwenden
  - Sonst `evictions`-Stat++, Item löschen, Chunk wiederverwenden
- [ ] LRU-Bump bei `get` (Item an Head der LRU bewegen)
- [ ] `expires_at`-Check bei jedem `get`/`gets`; Lazy-Expiry (nur beim Zugriff prüfen, kein Background-Thread in Phase 2)

**Done-Kriterium:** `stats slabs`, `stats items` liefern plausible Werte; bei
Volllaufen wird tatsächlich evicted; `reclaimed`/`evictions`-Stats steigen.

---

## Phase 3 — Statistics

memcached ohne Stats ist nicht nutzbar. Alle Stats müssen mit echten Werten
gefüllt werden.

### 3.1 `stats` (General)

Alle diese Stat-Namen müssen zurückgegeben werden (Auswahl der wichtigsten,
siehe protocol.txt für die komplette Liste):

- [ ] `pid`, `uptime`, `time`, `version`, `pointer_size`
- [ ] `rusage_user`, `rusage_system`
- [ ] `curr_items`, `total_items`, `bytes`
- [ ] `max_connections`, `curr_connections`, `total_connections`, `connection_structures`, `rejected_connections`
- [ ] `cmd_get`, `cmd_set`, `cmd_flush`, `cmd_touch`
- [ ] `get_hits`, `get_misses`, `get_expired`, `get_flushed`
- [ ] `delete_misses`, `delete_hits`
- [ ] `incr_misses`, `incr_hits`, `decr_misses`, `decr_hits`
- [ ] `cas_misses`, `cas_hits`, `cas_badval`
- [ ] `touch_hits`, `touch_misses`
- [ ] `store_too_large`, `store_no_memory`
- [ ] `auth_cmds`, `auth_errors`
- [ ] `idle_kicks`, `evictions`, `reclaimed`
- [ ] `bytes_read`, `bytes_written`, `limit_maxbytes`
- [ ] `accepting_conns`, `listen_disabled_num`, `threads`
- [ ] `hash_power_level`, `hash_bytes`, `hash_is_expanding`
- [ ] `expired_unfetched`, `evicted_unfetched`, `evicted_active`
- [ ] (Slab/LRU-spezifische Stats siehe 3.3/3.4)

### 3.2 `stats settings`

- [ ] Alle `stats settings`-Felder (maxbytes, maxconns, tcpport, udpport, inter,
      verbosity, evictions, growth_factor, chunk_size, num_threads, cas_enabled,
      item_size_max, hash_algorithm, lru_crawler, lru_maintainer_thread, …)

### 3.3 `stats slabs`

- [ ] Per-Slab: `chunk_size`, `chunks_per_page`, `total_pages`, `total_chunks`,
      `get_hits`, `cmd_set`, `delete_hits`, `incr_hits`, `decr_hits`, `cas_hits`,
      `cas_badval`, `touch_hits`, `used_chunks`, `free_chunks`, `free_chunks_end`
- [ ] Global: `active_slabs`, `total_malloced`

### 3.4 `stats items`

- [ ] Per-Slab: `number`, `number_hot/warm/cold/temp`, `age_hot/warm`, `age`,
      `mem_requested`, `evicted`, `evicted_nonzero`, `evicted_time`,
      `outofmemory`, `reclaimed`, `expired_unfetched`, `evicted_unfetched`,
      `evicted_active`, `crawler_reclaimed`, `moves_to_cold/warm`, `direct_reclaims`,
      `hits_to_hot/warm/cold/temp`

### 3.5 `stats sizes`

- [ ] Histogram (32-Byte-Buckets); nur aktivierbar via `-o track_sizes`;
      sonst `STAT sizes_status disabled`

### 3.6 `stats conns`

- [ ] Pro FD: `addr`, `listen_addr`, `state`, `secs_since_last_cmd`
- [ ] States: conn_closing, conn_listening, conn_mwrite, conn_new_cmd,
      conn_nread, conn_parse_cmd, conn_read, conn_swallow, conn_waiting, conn_write

### 3.7 `stats detail` / `stats detail on|off|dump`

- [ ] Key-Prefix-basierte Detail-Stats (optional, niedrigere Priorität)

**Done-Kriterium:** `stats`, `stats settings`, `stats slabs`, `stats items`
sind alle vorhanden und liefern korrekte, monoton wachsende Zähler.

---

## Phase 4 — Erweiterte LRU- und Slab-Verwaltung

### 4.1 Segmented LRU (HOT/WARM/COLD/TEMP)

- [ ] `-o lru_maintainer` / `-o modern` — drei LRU-Segmente pro Klasse
- [ ] HOT_LRU (% via `hot_lru_pct`), WARM_LRU (`warm_lru_pct`), COLD_LRU
- [ ] TEMP_LRU für Items mit `ttl < temporary_ttl` (`-o temp_lru=<ttl>`) — unevictable
- [ ] Background `lru_maintainer`-Thread:
  - Verschiebt Items HOT->COLD, COLD->WARM nach Age-Heuristik
  - `moves_to_cold`, `moves_to_warm`, `moves_within_lru` Stats
  - `lru_maintainer_juggles` Stat

### 4.2 LRU Crawler

- [ ] `lru_crawler <enable|disable>`
- [ ] `lru_crawler sleep <us>`
- [ ] `lru_crawler tocrawl <n>`
- [ ] `lru_crawler crawl <classids|all>` — aktiv expiriert Items vom Tail
- [ ] `lru_crawler metadump <classids|all|hash>` — Dump aller Keys (URI-encoded)
- [ ] `lru_crawler mgdump <classids|all|hash>` — Dump im `mg key`-Format
- [ ] Stats: `crawler_reclaimed`, `crawler_items_checked`, `lru_crawler_starts`

### 4.3 Slab Reassignment & Automove

- [ ] `slabs reassign <source> <dest>` — Page zwischen Klassen verschieben
  - Responses: `OK`/`BUSY`/`BADCLASS`/`NOSPARE`/`NOTFULL`/`UNSAFE`/`SAME`
- [ ] `slabs automove <0|1|2>` — Background-Automover
  - 0=standby, 1=return-to-pool-when-spare, 2=aggressive-on-eviction
- [ ] Stats: `slab_reassign_running`, `slabs_moved`, `slab_global_page_pool`,
      `slab_reassign_rescues`, `slab_reassign_busy_*`

### 4.4 LRU Tuning

- [ ] `lru tune <hot_pct> <warm_pct> <hot_max_factor> <warm_max_factor>`
- [ ] `lru mode <flat|segmented>`
- [ ] `lru temp_ttl <ttl>`

**Done-Kriterium:** Segmented LRU funktioniert, Crawler läuft, Slab-Reassign
kann Page verschieben.

---

## Phase 5 — Meta-Protokoll (mg/ms/md/ma/mn/me)

Das Meta-Protokoll ersetzt/vaildiert Binary und erweitert Basic-Text.
Es ist der „moderne" Weg.

### 5.1 Meta Get (`mg`)

Syntax: `mg <key> <flags>*\r\n`
Response: `VA <size> <flags>*\r\n<data>\r\n` | `HD <flags>*\r\n` | `EN\r\n`

- [ ] Flags: `b`(base64 key), `c`(return CAS), `C<tok>`(check CAS, skip value if match),
      `f`(client flags), `h`(hit-before), `k`(return key), `l`(last-access secs),
      `O<tok>`(opaque), `q`(noreply/quiet), `s`(item size), `t`(TTL remaining),
      `u`(don't bump LRU), `v`(return value)
- [ ] Modifizierende Flags: `E<tok>`(set CAS), `N<tok>`(vivify on miss + TTL),
      `R<tok>`(early-recache win), `T<tok>`(update TTL)
- [ ] Response-Flags: `W`(won recache), `X`(stale), `Z`(already-won)
- [ ] `P<tok>`/`L<tok>` werden ignoriert (Proxy-Hints)

### 5.2 Meta Set (`ms`)

Syntax: `ms <key> <datalen> <flags>*\r\n<data>\r\n`
Response: `HD`/`NS`/`EX`/`NF` [+ flags]

- [ ] Flags: `b`, `c`(return CAS), `C<tok>`(compare CAS), `E<tok>`(set CAS),
      `F<tok>`(client flags), `I`(invalidate/stale), `k`, `O<tok>`, `q`, `s`(return size),
      `T<tok>`(TTL), `M<tok>`(mode: E=add, A=append, P=prepend, R=replace, S=set),
      `N<tok>`(autovivify on append-miss)

### 5.3 Meta Delete (`md`)

Syntax: `md <key> <flags>*\r\n`
Response: `HD`/`NF`/`EX` [+ flags]

- [ ] Flags: `b`, `C<tok>`, `E<tok>`, `I`(mark stale + bump CAS), `k`, `O<tok>`,
      `q`, `T<tok>`(TTL mit `I`), `x`(remove value, keep item/tombstone)

### 5.4 Meta Arithmetic (`ma`)

Syntax: `ma <key> <flags>*\r\n`
Response: `HD`/`NF`/`NS`/`EX` oder `VA <size> <flags>\r\n<number>\r\n`

- [ ] Flags: `b`, `C<tok>`, `E<tok>`, `N<tok>`(autocreate + TTL), `J<tok>`(initial value),
      `D<tok>`(delta), `T<tok>`(update TTL), `M<tok>`(mode: I/+ incr, D/- decr),
      `O<tok>`, `q`, `t`(return TTL), `c`(return CAS), `v`(return value), `k`

### 5.5 Meta No-Op (`mn`)

- [ ] `mn\r\n` -> `MN\r\n` (für Pipelining-Terminierung)

### 5.6 Meta Debug (`me`)

- [ ] `me <key> [b]\r\n` -> `ME <key> <k>=<v>*\r\n` (exp, la, cas, fetch, cls, size, …)

**Done-Kriterium:** Meta-Kommandos sind funktionsfähig und kompatibel mit
existenten Meta-Clients (z. B. `memcached`-CLI-Tests).

---

## Phase 6 — Binary-Protokoll

> Offiziell _deprecated_, aber für 1:1-Kompatibilität erforderlich.

- [ ] 24-Byte Header-Parser (Request: magic=0x80, Response: magic=0x81)
  - Felder: magic, opcode, key_length, extras_length, data_type,
    vbucket_id/status, total_body_length, opaque, cas
- [ ] Opcodes: Get(0x00), Set(0x01), Add(0x02), Replace(0x03), Delete(0x04),
      Increment(0x05), Decrement(0x06), Quit(0x07), Flush(0x08), GetQ(0x09),
      No-op(0x0a), Version(0x0b), GetK(0x0c), GetKQ(0x0d), Append(0x0e),
      Prepend(0x0f), Stat(0x10), SetQ(0x11), AddQ(0x12), ReplaceQ(0x13),
      DeleteQ(0x14), IncrementQ(0x15), DecrementQ(0x16), QuitQ(0x17),
      FlushQ(0x18), AppendQ(0x19), PrependQ(0x1a), Verbosity(0x1b),
      Touch(0x1c), GAT(0x1d), GATQ(0x1e), SASL list mechs(0x20), SASL Auth(0x21),
      SASL Step(0x22)
- [ ] Quiet-Varianten (Q-Suffix) — suppress success responses
- [ ] Response Status Codes: 0x0000 No error, 0x0001 Key not found,
      0x0002 Key exists, 0x0003 Value too large, 0x0004 Invalid arguments,
      0x0005 Item not stored, 0x0006 Non-numeric value, 0x0081 Unknown command,
      0x0082 Out of memory, …
- [ ] Extras für Set/Add/Replace (8 Bytes: flags + expiration),
      Increment/Decrement (20 Bytes: delta + initial + expiration),
      Flush (4 Bytes: expiration), Touch/GAT (4 Bytes: expiration)
- [ ] Protokoll-Auto-Detection (Text vs. Binary) am ersten Byte (0x80 = Binary)

**Done-Kriterium:** Binary-Clients (z. B. `pylibmc`, `spymemcached`-Tests)
funktionieren. Text- und Binary-Verkehr auf dem selben Port.

---

## Phase 7 — Transports & Networking

### 7.1 UDP

- [ ] UDP-Frame-Header (8 Bytes): request_id, seq_num, total_datagrams, reserved
- [ ] UDP-Listener (separater Port `-U`, default off)
- [ ] Multi-Datagram-Response (Sequenz-Nummern)
- [ ] Nur geeignet für kleine Items/gets (wie memcached)

### 7.2 Unix Domain Sockets

- [ ] `-s <path>` aktiviert UDS, deaktiviert TCP/UDP
- [ ] `-a` umask (veraltetes `-a`), `-u` user für UDS-Perms

### 7.3 Connection-Limit & Threading

- [ ] `-c <max>` Connection-Limit (default 1024); bei Limit `listen_disabled_num`++,
      `rejected_connections`++ (im `maxconns_fast` Modus sofort abweisen)
- [ ] `-t <threads>` Worker-Threads (default 4)
  - Main-Thread akzeptiert, verteilt per round-robin (oder NAPI-IDs)
  - Pro Thread eigenes Event-Loop (tokio: `LocalSet` für Pinned Tasks)
  - `conn_yields` bei `-R` reqs-per-event-Limit

### 7.4 Idle-Timeout

- [ ] `-o idle_time=<secs>` — Verbindungen nach N Sekunden Inaktivität kicken (`idle_kicks`)

**Done-Kriterium:** UDP-gets funktionieren; UDS nutzbar; Connection-Limit greift.

---

## Phase 8 — Security & Auth

### 8.1 SASL Authentication

- [ ] `-Y` SASL (PLAIN, optional CRAM-MD5)
- [ ] Binary: SASL list mechs / SASL Auth / SASL Step
- [ ] Text: Fake-`set`-Auth (key egal, bytes = Länge von `username password`)
  - `STORED` bei Erfolg, `CLIENT_ERROR` bei Fehlschlag
- [ ] Stats: `auth_cmds`, `auth_errors`, `auth_enabled_sasl` setting

### 8.2 TLS

- [ ] `-o ssl_chain_cert=<path>`, `-o ssl_key=<path>`, `-Z` enable TLS
- [ ] TLS-Stats: `ssl_handshake_errors`, `ssl_proto_errors`, `ssl_min_version`,
      `ssl_new_sessions`, `time_since_server_cert_refresh`
- [ ] Zertifikat-Hot-Reload

### 8.3 Privilege Dropping / Seccomp

- [ ] `-u <user>` — nach Bind zu unprivilegiertem User wechseln
- [ ] `-o drop_privileges` — seccomp (Linux) / pledge (OpenBSD)
- [ ] `misbehave`-Befehl (nur Debug-Build) zum Testen der Restrictions

**Done-Kriterium:** SASL-Login funktioniert (Text & Binary); TLS-Handshake
erfolgreich; Privileges gedropped.

---

## Phase 9 — Watchers (Observability)

- [ ] `watch fetchers|mutations|evictions|connevents|proxyreqs|proxyevents|proxyuser|deletions`
- [ ] Verbindung wird zum Watcher, erhält Log-Events im `key=value`-Format
- [ ] `ts=` timestamp, `gid=` global log id, URI-encoded values
- [ ] Stats: `log_worker_dropped`, `log_worker_written`,
      `log_watcher_skipped`, `log_watcher_sent`, `log_watchers`

**Done-Kriterium:** `watch evictions` zeigt Live-Evictions an.

---

## Phase 10 — Erweiterte Features (Spezial)

### 10.1 Warm Restart / External Memory

- [ ] `-o memory_file=<path>` — Speicher auf Datei persistieren für Warm-Restart
- [ ] Bei Shutdown: Dump aller Items in Datei
- [ ] Bei Start: Datei einlesen und Slabs/Items rekonstruieren

### 10.2 Large Items (Extended Page)

- [ ] `-o slab_chunk_max=<bytes>` — größere Slab-Klassen
- [ ] Items > 1MB über Multi-Chunk-Allokation

### 10.3 NAPI IDs (Linux)

- [ ] `-o napi_ids` — Thread-Affinität via NIC NAPI-IDs
- [ ] Stats: `unexpected_napi_ids`, `round_robin_fallback`

### 10.4 Response-Buffer-Management

- [ ] `read_obj_mem_limit` — Memory-Budget für conn-read/resp buffers
- [ ] Stats: `response_obj_oom`, `response_obj_count`, `response_obj_bytes`,
      `read_buf_count`, `read_buf_bytes`, `read_buf_bytes_free`, `read_buf_oom`

### 10.5 Stats-Prefix / Detail

- [ ] `-o stat_key_prefix=<char>` — Prefix-Char für Stats-Detail-Keys
- [ ] `stats detail on|off|dump` — per-Prefix-Counter

### 10.6 Proxy-Modus (optional, sehr fortgeschritten)

- [ ] `-o proxy_config=<file>` — Lua-basierte Proxy-Konfiguration
- [ ] `proxy_enabled`/`proxy_uring_enabled` settings
- [ ] Stats: `proxy_conn_requests`, `proxy_conn_errors`, `proxy_conn_oom`,
      `proxy_req_active`
- [ ] Hinweis: extrem komplex — erst ganz am Ende oder optional weglassen

**Done-Kriterium:** Warm-Restart funktioniert; große Items speicherbar.

---

## Quellen / Referenzmaterial

- **Text/Meta-Protokoll:** https://github.com/memcached/memcached/blob/master/doc/protocol.txt
- **Binary-Protokoll:** https://docs.memcached.org/protocols/binary/
- **Konfiguration:** https://docs.memcached.org/serverguide/configuring/
- **Meta-Wiki:** https://github.com/memcached/memcached/wiki/MetaCommands
- **Original-Source:** https://github.com/memcached/memcached (C-Referenz für Verhalten)

## Empfohlene Test-Strategie

1. **Phase 1-3:** `nc`-Skripte + manuelle `telnet`-Sessions
2. **Ab Phase 2:** Eigene Integration-Tests (z. B. `testcontainers` mit echtem
   memcached als „Golden Master")
3. **Ab Phase 5/6:** Bestehende Client-Libs (Python `pymemcache`, Go `gomemcache`,
   Rust `memcache`-crate) gegen memcrabd laufen lassen
4. **Ab Phase 4:** Belastungstest (z. B. `memtier_benchmark`, `twemperf`) und
   Vergleich der `stats`-Output-Zeilen mit echtem memcached
