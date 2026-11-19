"""Query published advisories for registry packages; report maintenance separately."""
import concurrent.futures
import json
from pathlib import Path
import sys
import tomllib
import urllib.request

lock=tomllib.loads(Path('src-tauri/Cargo.lock').read_text())
queries=[{'package':{'name':p['name'],'ecosystem':'crates.io'},'version':p['version']}
    for p in lock['package'] if p.get('source','').startswith('registry+')]
findings=[]
for offset in range(0,len(queries),200):
    batch=queries[offset:offset+200]
    request=urllib.request.Request('https://api.osv.dev/v1/querybatch',data=json.dumps({'queries':batch}).encode(),headers={'Content-Type':'application/json'})
    with urllib.request.urlopen(request,timeout=60) as response:rows=json.load(response)['results']
    findings.extend({'package':q['package']['name'],'version':q['version'],'advisories':r['vulns']}
        for q,r in zip(batch,rows) if r.get('vulns'))
ids={a['id'] for row in findings for a in row['advisories']}
def read(identifier):
    with urllib.request.urlopen('https://api.osv.dev/v1/vulns/'+identifier,timeout=30) as response:return identifier,json.load(response)
with concurrent.futures.ThreadPoolExecutor(6) as pool:advisories=dict(pool.map(read,ids))
maintenance=[];vulnerabilities=[]
for row in findings:
    for advisory in row['advisories']:
        details=advisories[advisory['id']]
        item={'package':row['package'],'version':row['version'],'id':advisory['id'],'summary':details.get('summary')}
        if details.get('database_specific',{}).get('informational')=='unmaintained' or 'unmaintained' in details.get('summary','').lower():maintenance.append(item)
        elif not details.get('withdrawn'):vulnerabilities.append(item)
# The GTK 3 dependency is a local source backport, not an unmodified registry crate.
source=Path('src-tauri/vendor/glib/src/variant_iter.rs').read_text()
assert 'let mut p: *mut libc::c_char = std::ptr::null_mut();' in source and '                &mut p,' in source
print(json.dumps({'registry_packages_checked':len(queries),'vulnerabilities':vulnerabilities,'maintenance_warnings':maintenance,
    'local_security_backport':{'package':'glib 0.18.5','advisory':'RUSTSEC-2024-0429','upstream_fix':'gtk-rs/gtk-rs-core#1343','regression':'optimized Linux variant_string_iteration_has_valid_forward_and_reverse_pointers'}},indent=2))
sys.exit(bool(vulnerabilities))
