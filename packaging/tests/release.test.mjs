import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp,writeFile,readFile,mkdir,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {createHash} from 'node:crypto';
import {packageNames,versionKey,readMetadata,writeChecksums,validatePackages,prepareVersion,releaseNotes,releaseTitle,assertNativeTarget,verifyPublishedRelease} from '../release.mjs';

async function fixture(t){const dir=await mkdtemp(join(tmpdir(),'santacita-release-'));t.after(()=>rm(dir,{recursive:true,force:true}));return dir;}
test('package matrix has five exact public names including portable',()=>{
 assert.deepEqual(packageNames('0.4.0-beta.1'),['SantaCita-0.4.0-beta.1-macOS-Apple-Silicon.dmg','SantaCita-0.4.0-beta.1-macOS-Intel.dmg','SantaCita-0.4.0-beta.1-windows-x64.msi','SantaCita-0.4.0-beta.1-windows-x64-portable.zip','SantaCita-0.4.0-beta.1-x86_64.AppImage']);
 assert.throws(()=>packageNames('0.4.0-beta.01'));
});
test('beta versions sort numerically and precede their stable version',()=>{
 assert(versionKey('0.4.0-beta.10')>versionKey('0.4.0-beta.2'));
 assert(versionKey('0.4.0')>versionKey('0.4.0-beta.10'));
 for(const bad of ['0.4.0-beta.0','0.4.0-rc.1','00.4.0','0.4.0/evil'])assert.throws(()=>versionKey(bad));
});
test('metadata rejects stale frontend or wrong tag before building',async t=>{
 const root=await fixture(t);await mkdir(join(root,'desktop-ui'));await mkdir(join(root,'.github'));
 await writeFile(join(root,'.github/release-version'),'0.4.0-beta.1\n');
 await writeFile(join(root,'Cargo.toml'),'[workspace.package]\nversion = "0.4.0-beta.1"\n');
 await writeFile(join(root,'Cargo.lock'),'[[package]]\nname = "ceh-core"\nversion = "0.4.0-beta.1"\n');
 await writeFile(join(root,'desktop-ui/package.json'),JSON.stringify({version:'0.4.0-beta.1'}));
 await writeFile(join(root,'desktop-ui/package-lock.json'),JSON.stringify({version:'0.4.0-beta.1',packages:{'':{version:'0.4.0-beta.1'}}}));
 assert.equal((await readMetadata(root,'v0.4.0-beta.1')).version,'0.4.0-beta.1');
 await assert.rejects(readMetadata(root,'v0.4.0-beta.2'));
 await writeFile(join(root,'desktop-ui/package.json'),JSON.stringify({version:'0.3.8'}));
 await assert.rejects(readMetadata(root));
});
test('checksums cover all packages and detect corruption and missing packages',async t=>{
 const dir=await fixture(t);const names=packageNames('0.4.0-beta.1');
 for(const name of names)await writeFile(join(dir,name),'synthetic package');
 await writeChecksums(dir,names);
 const sums=await readFile(join(dir,'SHA256SUMS'),'utf8');
 const sha=createHash('sha256').update('synthetic package').digest('hex');
 assert.equal(sums.split('\n')[0],sha+'  '+names[0]);
 await validatePackages(dir,'0.4.0-beta.1',{proofs:false});
 await assert.rejects(validatePackages(dir,'0.4.0-beta.1',{proofs:true}));
 await writeFile(join(dir,names[0]),'tampered');
 await assert.rejects(validatePackages(dir,'0.4.0-beta.1',{proofs:false}));
 await rm(join(dir,names[0]));
 await assert.rejects(validatePackages(dir,'0.4.0-beta.1',{proofs:false}));
});

test('release preparation synchronizes source and notes and is safe to repeat',async t=>{
 const root=await fixture(t);await mkdir(join(root,'desktop-ui'));await mkdir(join(root,'.github'));
 await writeFile(join(root,'.github/release-version'),'0.4.0-beta.2\n');
 await writeFile(join(root,'Cargo.toml'),'[workspace.package]\nversion = "0.4.0-beta.1"\n');
 await writeFile(join(root,'Cargo.lock'),'[[package]]\nname = "ceh-core"\nversion = "0.4.0-beta.1"\n');
 await writeFile(join(root,'desktop-ui/package.json'),JSON.stringify({version:'0.4.0-beta.1'}));
 await writeFile(join(root,'desktop-ui/package-lock.json'),JSON.stringify({version:'0.4.0-beta.1',packages:{'':{version:'0.4.0-beta.1'}}}));
 await writeFile(join(root,'CHANGELOG.md'),'# Changelog\n\n## Unreleased — Beta fixes\n\n- Keep imported profiles.\n\n## 0.3.8 — Python bridge\n\n- Prepare updates.\n');
 await prepareVersion(root);
 assert.equal((await readMetadata(root)).version,'0.4.0-beta.2');
 assert.deepEqual(await releaseNotes(root,'0.4.0-beta.2'),{title:'Beta fixes',body:'- Keep imported profiles.'});
 const before=await readFile(join(root,'CHANGELOG.md'),'utf8');await prepareVersion(root);assert.equal(await readFile(join(root,'CHANGELOG.md'),'utf8'),before);
 await writeFile(join(root,'.github/release-version'),'0.4.0-beta.1\n');await assert.rejects(prepareVersion(root));
});

test('after the stable 0.4.0 no 0.4.0 beta can be prepared, while a 0.5.0 beta can (a beta never outranks a stable)',async t=>{
 const root=await fixture(t);await mkdir(join(root,'desktop-ui'));await mkdir(join(root,'.github'));
 await writeFile(join(root,'Cargo.toml'),'[workspace.package]\nversion = "0.4.0"\n');
 await writeFile(join(root,'Cargo.lock'),'[[package]]\nname = "ceh-core"\nversion = "0.4.0"\n');
 await writeFile(join(root,'desktop-ui/package.json'),JSON.stringify({version:'0.4.0'}));
 await writeFile(join(root,'desktop-ui/package-lock.json'),JSON.stringify({version:'0.4.0',packages:{'':{version:'0.4.0'}}}));
 await writeFile(join(root,'CHANGELOG.md'),'# Changelog\n\n## Unreleased — Next\n\n- Something new.\n\n## 0.4.0 — First release\n\n- Everything.\n');
 await writeFile(join(root,'.github/release-version'),'0.4.0-beta.3\n');
 await assert.rejects(prepareVersion(root),/backwards/);
 await writeFile(join(root,'.github/release-version'),'0.5.0-beta.1\n');
 await prepareVersion(root);
 assert.equal((await readMetadata(root)).version,'0.5.0-beta.1');
 assert(versionKey('0.5.0-beta.1')>versionKey('0.4.0'));
});
test('Windows CRLF metadata is accepted and the declared tag version must agree',async t=>{
 const root=await fixture(t);await mkdir(join(root,'desktop-ui'));await mkdir(join(root,'.github'));
 await writeFile(join(root,'.github/release-version'),'0.4.0-beta.2\r\n');
 await writeFile(join(root,'Cargo.toml'),'[workspace.package]\r\nversion = "0.4.0-beta.1"\r\n');
 await writeFile(join(root,'Cargo.lock'),'[[package]]\r\nname = "ceh-core"\r\nversion = "0.4.0-beta.1"\r\n');
 await writeFile(join(root,'desktop-ui/package.json'),JSON.stringify({version:'0.4.0-beta.1'}));
 await writeFile(join(root,'desktop-ui/package-lock.json'),JSON.stringify({version:'0.4.0-beta.1',packages:{'':{version:'0.4.0-beta.1'}}}));
 await writeFile(join(root,'CHANGELOG.md'),'# Changelog\r\n\r\n## Unreleased — Windows fixes\r\n\r\n- Keep upgrades working.\r\n');
 assert.equal((await readMetadata(root)).version,'0.4.0-beta.1');
 await assert.rejects(readMetadata(root,'v0.4.0-beta.1'));
 await prepareVersion(root);
 assert.equal((await readMetadata(root,'v0.4.0-beta.2')).version,'0.4.0-beta.2');
 assert.deepEqual(await releaseNotes(root,'0.4.0-beta.2'),{title:'Windows fixes',body:'- Keep upgrades working.'});
});
test('package CPU must match the explicit native compiler target',()=>{
 assertNativeTarget('macos','arm64','darwin','aarch64-apple-darwin');
 assertNativeTarget('windows','x64','win32','x86_64-pc-windows-msvc');
 assert.throws(()=>assertNativeTarget('macos','x64','darwin','aarch64-apple-darwin'));
 assert.throws(()=>assertNativeTarget('linux','x64','darwin','x86_64-unknown-linux-gnu'));
});
test('published releases must be immutable, complete and have matching remote digests',async t=>{
 const dir=await fixture(t);const version='0.4.0-beta.1';const names=packageNames(version);
 const files=names.flatMap(name=>[name,`${name}.sigstore.json`]).concat('SHA256SUMS');
 for(const name of files)await writeFile(join(dir,name),'public fixture');
 const hash=createHash('sha256').update('public fixture').digest('hex');
 const published={tag_name:'v0.4.0-beta.1',draft:false,prerelease:true,immutable:true,assets:files.map(name=>({name,state:'uploaded',digest:`sha256:${hash}`}))};
 await verifyPublishedRelease(published,dir,version);
 await assert.rejects(verifyPublishedRelease({...published,immutable:false},dir,version));
 await assert.rejects(verifyPublishedRelease({...published,assets:published.assets.slice(1)},dir,version));
 await assert.rejects(verifyPublishedRelease({...published,prerelease:false},dir,version));
 published.assets[0].digest='sha256:'+'0'.repeat(64);
 await assert.rejects(verifyPublishedRelease(published,dir,version));
});

test('the MSI installs WebView2 with Microsoft\'s signed bootstrapper only when no runtime is registered, and releases require it',async()=>{
 const wxs=await readFile(new URL('../windows/SantaCita.wxs',import.meta.url),'utf8');
 const script=await readFile(new URL('../windows/package.ps1',import.meta.url),'utf8');
 const workflow=await readFile(new URL('../../.github/workflows/build.yml',import.meta.url),'utf8');
 assert.match(wxs,/F3017226-FE2A-4295-8BDF-00C3A9A7E4C5/);
 // NOT REMOVE (not NOT Installed): a repair retries after an offline install. A registered version of 0.0.0.0 means "no runtime" (Microsoft's own rule).
 assert.match(wxs,/Condition="NOT REMOVE AND \(NOT WEBVIEW2_MACHINE OR WEBVIEW2_MACHINE = &quot;0\.0\.0\.0&quot;\) AND \(NOT WEBVIEW2_USER OR WEBVIEW2_USER = &quot;0\.0\.0\.0&quot;\)"/);
 assert.match(wxs,/Impersonate="yes"/,'runs as the installing user, so a per-user MSI stays free of administrator rights');
 assert.match(wxs,/Return="ignore"/,'a missing network never fails the installation');
 assert.match(script,/Get-AuthenticodeSignature/);assert.match(script,/O=Microsoft Corporation/);
 assert.match(script,/CEH_REQUIRE_WEBVIEW2_SETUP/);
 assert.match(workflow,/CEH_REQUIRE_WEBVIEW2_SETUP=1/,'a release build cannot skip the embedded installer');
});
test('a release without a title is just its version in the notes and in the release name',async t=>{
 const root=await fixture(t);await mkdir(join(root,'desktop-ui'));await mkdir(join(root,'.github'));
 await writeFile(join(root,'.github/release-version'),'0.4.3\n');
 await writeFile(join(root,'Cargo.toml'),'[workspace.package]\nversion = "0.4.2"\n');
 await writeFile(join(root,'Cargo.lock'),'[[package]]\nname = "ceh-core"\nversion = "0.4.2"\n');
 await writeFile(join(root,'desktop-ui/package.json'),JSON.stringify({version:'0.4.2'}));
 await writeFile(join(root,'desktop-ui/package-lock.json'),JSON.stringify({version:'0.4.2',packages:{'':{version:'0.4.2'}}}));
 await writeFile(join(root,'CHANGELOG.md'),'# Changelog\n\n## Unreleased\n\n- Something new.\n\n## 0.4.2 — Old title\n\n- Older.\n');
 await prepareVersion(root);
 const changelog=await readFile(join(root,'CHANGELOG.md'),'utf8');
 assert.match(changelog,/^## 0\.4\.3\n/m);assert.doesNotMatch(changelog,/0\.4\.3 —/);
 assert.deepEqual(await releaseNotes(root,'0.4.3'),{title:'',body:'- Something new.'});
 assert.deepEqual(await releaseNotes(root,'0.4.2'),{title:'Old title',body:'- Older.'});
 assert.equal(releaseTitle('0.4.3',''),'SantaCita 0.4.3');
 assert.equal(releaseTitle('0.4.2','Old title'),'SantaCita 0.4.2 — Old title');
 const before=changelog;await prepareVersion(root);assert.equal(await readFile(join(root,'CHANGELOG.md'),'utf8'),before);
});
