#!/usr/bin/env python3
"""Preprocess AIT-LDSv2 scenarios into fishy JSON collections.

Usage:
    python3 scripts/prep_ait.py data/ait/russellmitchell

Reads gather/<host>/logs/*, splits events by attack window into
baseline and test collections, writes fishy JSON directly.

Each (host, log_group) pair becomes one source.
"""

import json, os, re, sys
from collections import Counter, defaultdict
from datetime import datetime, timezone
from pathlib import Path

# ---------------------------------------------------------------------------
# Timestamp parsing
# ---------------------------------------------------------------------------

SYSLOG_RE = re.compile(r'^(\w{3}\s+\d+\s+\d+:\d+:\d+)\s+\S+\s+(\S+?)(?:\[\d+\])?:\s+(.+)$')
APACHE_ACCESS_RE = re.compile(r'^(?:\S+\s+)?(\S+)\s+\S+\s+\S+\s+\[([^\]]+)\]\s+"(\S+)\s+(\S+)[^"]*"\s+(\d+)')
APACHE_ERROR_RE = re.compile(r'^\[(\w+ \w+ +\d+ [\d:.]+\s+\d+)\]\s+\[([^\]]+)\]\s+(?:\[pid \d+\]\s+)?(?:\[client [^\]]+\]\s+)?(.+)$')
AUDIT_RE = re.compile(r'type=(\S+)\s+msg=audit\((\d+)\.\d+:\d+\)')
OPENVPN_RE = re.compile(r'^(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2})\s+(.+)$')

MONTHS = {'Jan':1,'Feb':2,'Mar':3,'Apr':4,'May':5,'Jun':6,
           'Jul':7,'Aug':8,'Sep':9,'Oct':10,'Nov':11,'Dec':12}

def parse_syslog_ts(ts_str, year=2022):
    """Parse 'Jan 21 00:00:01' → unix seconds."""
    parts = ts_str.split()
    if len(parts) < 3: return None
    m = MONTHS.get(parts[0])
    if not m: return None
    d = int(parts[1])
    h, mi, s = (int(x) for x in parts[2].split(':'))
    try:
        dt = datetime(year, m, d, h, mi, s, tzinfo=timezone.utc)
        return int(dt.timestamp())
    except: return None

def parse_apache_ts(ts_str):
    """Parse '21/Jan/2022:00:00:01 +0000' → unix seconds."""
    try:
        dt = datetime.strptime(ts_str.split()[0], '%d/%b/%Y:%H:%M:%S')
        dt = dt.replace(tzinfo=timezone.utc)
        return int(dt.timestamp())
    except: return None

def parse_apache_error_ts(ts_str):
    """Parse 'Fri Jan 21 00:00:01.123456 2022' → unix seconds."""
    parts = ts_str.split()
    if len(parts) < 5: return None
    m = MONTHS.get(parts[1])
    if not m: return None
    d = int(parts[2])
    time_part = parts[3].split('.')[0]
    h, mi, s = (int(x) for x in time_part.split(':'))
    y = int(parts[4])
    try:
        dt = datetime(y, m, d, h, mi, s, tzinfo=timezone.utc)
        return int(dt.timestamp())
    except: return None

def parse_iso_ts(ts_str):
    """Parse '2022-01-21T00:00:01.123456+0000' → unix seconds."""
    try:
        clean = re.sub(r'\.\d+', '', ts_str)
        clean = re.sub(r'\+(\d{2})(\d{2})$', r'+\1:\2', clean)
        dt = datetime.fromisoformat(clean)
        return int(dt.timestamp())
    except: return None

# ---------------------------------------------------------------------------
# Normalisation (mirrors encoder logic)
# ---------------------------------------------------------------------------

NORM_RE = re.compile(
    r'\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b'
    r'|\b[0-9a-fA-F]{8,}\b'
    r'|\b\d+\b'
    r'|/[\w./\-]+'
)

def norm(msg):
    return NORM_RE.sub('<v>', msg)

def norm_path(path):
    return re.sub(r'/\d+(?:/|$)|[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}|\?.*$', '/<id>', path)

# ---------------------------------------------------------------------------
# Line parsers → (template, unix_ts) or None
# ---------------------------------------------------------------------------

def parse_syslog_line(line):
    m = SYSLOG_RE.match(line)
    if not m: return None
    ts = parse_syslog_ts(m.group(1))
    proc = m.group(2)
    msg = norm(m.group(3))
    return (f'{proc}: {msg}', ts)

def parse_apache_access_line(line):
    m = APACHE_ACCESS_RE.match(line)
    if not m: return None
    ts = parse_apache_ts(m.group(2))
    method = m.group(3)
    path = norm_path(m.group(4))
    status = m.group(5)
    return (f'{method} {path} {status}', ts)

def parse_apache_error_line(line):
    m = APACHE_ERROR_RE.match(line)
    if not m: return None
    ts = parse_apache_error_ts(m.group(1))
    level = m.group(2)
    msg = norm(m.group(3))
    return (f'{level}: {msg}', ts)

def parse_audit_line(line):
    m = AUDIT_RE.match(line)
    if not m: return None
    ts = int(m.group(2))
    typ = m.group(1)
    msg = norm(line)
    return (f'{typ}: {msg}', ts)

def parse_openvpn_line(line):
    m = OPENVPN_RE.match(line)
    if not m: return None
    ts = parse_iso_ts(m.group(1).replace(' ', 'T') + '+0000')
    msg = norm(m.group(2))
    return (f'openvpn: {msg}', ts)

def parse_suricata_line(line):
    try:
        d = json.loads(line)
    except: return None
    ts = parse_iso_ts(d.get('timestamp', ''))
    etype = d.get('event_type', '')
    if etype == 'stats': return None  # skip periodic stats
    if etype == 'alert':
        sig = d.get('alert', {}).get('signature', 'unknown')
        return (f'alert: {norm(sig)}', ts)
    return (f'{etype}', ts)

def parse_dnsmasq_line(line):
    # dnsmasq uses syslog format
    return parse_syslog_line(line)

# ---------------------------------------------------------------------------
# File → parser mapping
# ---------------------------------------------------------------------------

def get_parser_and_group(host, relpath):
    """Return (parser_fn, group_name) or None to skip."""
    name = relpath.name
    parts = str(relpath).split('/')

    # Skip non-log files
    if name.endswith(('.pcap', '.pdf', '.zip', '.journal')):
        return None
    if 'journal' in parts:
        return None
    if name in ('sm.log', 'attacks.log'):
        return None
    if 'downloads' in parts or 'configs' in parts or 'redis' in parts:
        return None
    if name.startswith('suricata') or name == 'fast.log' or name == 'stats.log':
        if name != 'eve.json':
            return None

    # Suricata
    if name == 'eve.json' and 'suricata' in parts:
        return (parse_suricata_line, 'suricata')

    # Apache access
    if 'apache2' in parts and ('access' in name or 'vhosts' in name):
        return (parse_apache_access_line, 'apache_access')

    # Apache error
    if 'apache2' in parts and 'error' in name:
        return (parse_apache_error_line, 'apache_error')

    # Audit
    if name == 'audit.log' or ('audit' in parts and name.endswith('.log')):
        return (parse_audit_line, 'audit')

    # OpenVPN
    if 'openvpn' in name:
        return (parse_openvpn_line, 'openvpn')

    # dnsmasq
    if 'dnsmasq' in name:
        return (parse_dnsmasq_line, 'dnsmasq')

    # Horde
    if 'horde' in parts:
        return None  # skip for now

    # Exim
    if 'exim4' in parts:
        return (parse_syslog_line, 'exim')

    # Everything else: syslog-ish (auth, syslog, messages, mail.*, user.log)
    base = re.sub(r'\.\d+$', '', name)  # strip rotation suffix
    return (parse_syslog_line, base)


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    if len(sys.argv) < 2:
        print(f'usage: {sys.argv[0]} <scenario_dir>')
        sys.exit(1)

    scenario_dir = Path(sys.argv[1])
    gather_dir = scenario_dir / 'gather'
    if not gather_dir.exists():
        print(f'error: {gather_dir} not found')
        sys.exit(1)

    # Read dataset.yaml for simulation times
    ds_yaml = scenario_dir / 'dataset.yaml'
    sim_start = sim_end = attack_start = attack_end = None
    if ds_yaml.exists():
        with open(ds_yaml) as f:
            for line in f:
                if line.startswith('start:'):
                    sim_start = parse_iso_ts(line.split("'")[1] + '+00:00')
                elif line.startswith('end:'):
                    sim_end = parse_iso_ts(line.split("'")[1] + '+00:00')

    # Attack times (hardcoded per scenario from Zenodo description)
    scenario_name = scenario_dir.name
    ATTACKS = {
        'fox':              ('2022-01-18T11:59:00+00:00', '2022-01-18T13:15:00+00:00'),
        'harrison':         ('2022-02-08T07:07:00+00:00', '2022-02-08T08:38:00+00:00'),
        'russellmitchell':  ('2022-01-24T03:01:00+00:00', '2022-01-24T04:39:00+00:00'),
        'santos':           ('2022-01-17T11:15:00+00:00', '2022-01-17T11:59:00+00:00'),
        'shaw':             ('2022-01-29T14:37:00+00:00', '2022-01-29T15:21:00+00:00'),
        'wardbeck':         ('2022-01-23T12:10:00+00:00', '2022-01-23T12:56:00+00:00'),
        'wheeler':          ('2022-01-30T07:35:00+00:00', '2022-01-30T17:53:00+00:00'),
        'wilson':           ('2022-02-07T10:57:00+00:00', '2022-02-07T11:49:00+00:00'),
    }
    if scenario_name not in ATTACKS:
        print(f'error: unknown scenario {scenario_name}')
        sys.exit(1)

    attack_start = parse_iso_ts(ATTACKS[scenario_name][0])
    attack_end = parse_iso_ts(ATTACKS[scenario_name][1])
    attack_dur = attack_end - attack_start
    # Normal test window: same duration, ending at attack start
    normal_start = attack_start - attack_dur
    normal_end = attack_start

    print(f'scenario: {scenario_name}')
    print(f'simulation: {sim_start} – {sim_end} ({(sim_end-sim_start)/86400:.1f} days)')
    print(f'attack window: {attack_start} – {attack_end} ({attack_dur}s = {attack_dur/60:.0f} min)')
    print(f'normal test window: {normal_start} – {normal_end} ({attack_dur/60:.0f} min)')

    # Discover hosts
    hosts = sorted([d.name for d in gather_dir.iterdir() if d.is_dir()])
    # Skip attacker and external users
    hosts = [h for h in hosts if not h.startswith('attacker') and not h.startswith('ext_user')]
    print(f'hosts: {len(hosts)} — {", ".join(hosts)}')

    # Collect events per source
    # source_key = (host, group)
    # events = [(template, unix_ts)]
    sources = defaultdict(list)
    skipped_files = []
    total_lines = 0
    total_parsed = 0

    for host in hosts:
        logs_dir = gather_dir / host / 'logs'
        if not logs_dir.exists():
            continue
        for log_file in sorted(logs_dir.rglob('*')):
            if not log_file.is_file():
                continue
            relpath = log_file.relative_to(logs_dir)
            result = get_parser_and_group(host, relpath)
            if result is None:
                skipped_files.append(str(log_file))
                continue
            parser_fn, group = result
            source_key = f'{host}_{group}'

            try:
                with open(log_file, errors='replace') as f:
                    for line in f:
                        total_lines += 1
                        line = line.rstrip('\n')
                        parsed = parser_fn(line)
                        if parsed is None:
                            continue
                        template, ts = parsed
                        if ts is None:
                            continue
                        # Filter to simulation window
                        if sim_start and ts < sim_start:
                            continue
                        if sim_end and ts > sim_end:
                            continue
                        total_parsed += 1
                        sources[source_key].append((template, ts))
            except Exception as e:
                print(f'  warning: {log_file}: {e}')

    print(f'\ntotal lines read: {total_lines:,}')
    print(f'total events parsed: {total_parsed:,}')
    print(f'sources: {len(sources)}')
    print(f'skipped files: {len(skipped_files)}')

    # Filter sources with too few events
    MIN_EVENTS = 32
    sources = {k: v for k, v in sources.items() if len(v) >= MIN_EVENTS}
    print(f'sources after filtering (≥{MIN_EVENTS} events): {len(sources)}')

    # Build dictionary from all events
    freqs = Counter()
    for events in sources.values():
        for template, _ in events:
            freqs[template] += 1
    # Rank by frequency (most frequent = TemplateId 1)
    ranked = sorted(freqs.items(), key=lambda x: -x[1])
    template_to_id = {t: i+1 for i, (t, _) in enumerate(ranked)}
    print(f'dictionary: {len(template_to_id)} templates')

    # Assign consistent source IDs across all collections (baseline defines the mapping)
    all_keys = sorted(sources.keys())
    global_id_map = {key: i for i, key in enumerate(all_keys)}

    def make_collection(sources, ts_start, ts_end):
        coll_sources = {}
        id_map = {}
        for source_key in all_keys:
            if source_key not in sources:
                continue
            events = [(t, ts) for t, ts in sources[source_key] if ts_start <= ts < ts_end]
            if len(events) < MIN_EVENTS:
                continue
            events.sort(key=lambda x: x[1])
            sid = global_id_map[source_key]
            coll_sources[sid] = {
                'events': [
                    {'template_id': template_to_id[t], 'timestamp': ts - ts_start, 'params': {}}
                    for t, ts in events
                ]
            }
            id_map[sid] = source_key
        return coll_sources, id_map, ts_end - ts_start

    # Day-level splits (equal duration, good source coverage)
    n_days = (sim_end - sim_start) // 86400
    attack_day = (attack_start - sim_start) // 86400  # 0-indexed

    all_colls = []
    for d in range(n_days):
        d_start = sim_start + d * 86400
        d_end = d_start + 86400
        ds, dm, dd = make_collection(sources, d_start, d_end)
        label = f'day_{d}'
        all_colls.append((label, ds, dd, dm))

    print(f'\nday-level splits ({n_days} days, attack on day {attack_day}):')
    for name, ds, dd, _ in all_colls:
        print(f'  {name}: {len(ds)} sources, duration {dd}s ({dd/3600:.0f}h)')

    # Write collections
    out_base = scenario_dir / 'collections'
    for coll_name, coll_sources, coll_dur, id_map in all_colls:
        coll_dir = out_base / coll_name
        coll_dir.mkdir(parents=True, exist_ok=True)
        meta = {'start_time': 0, 'end_time': coll_dur}
        with open(coll_dir / 'meta.json', 'w') as f:
            json.dump(meta, f)
        for sid, stream in coll_sources.items():
            with open(coll_dir / f'{sid}.json', 'w') as f:
                json.dump(stream, f)
        # Write source map for reference (outside collection dir)
        with open(out_base / f'{coll_name}_source_map.json', 'w') as f:
            json.dump({str(k): v for k, v in id_map.items()}, f, indent=2)
        total_events = sum(len(s['events']) for s in coll_sources.values())
        print(f'  {coll_name}: {len(coll_sources)} sources, {total_events:,} events → {coll_dir}')

    # Save dictionary
    dict_path = scenario_dir / 'dict.json'
    with open(dict_path, 'w') as f:
        json.dump({'templates': {str(v): k for k, v in template_to_id.items()}}, f)
    print(f'\ndictionary: {len(template_to_id)} templates → {dict_path}')
    print(f'\nattack is on day {attack_day} — compare day_0 vs day_{attack_day} (attack) and day_0 vs day_{attack_day-1} (normal)')

if __name__ == '__main__':
    main()
