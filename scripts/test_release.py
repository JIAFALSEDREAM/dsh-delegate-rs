import contextlib
import fnmatch
import hashlib
import io
import json
import posixpath
import re
import runpy
from pathlib import Path
import subprocess
import tarfile
import tempfile
import tomllib
import unittest
import urllib.error
import zipfile
from unittest.mock import patch

import release


class DistributionDocumentsTests(unittest.TestCase):
    def test_public_document_links_are_shipped_in_every_distribution(self):
        root = Path(__file__).resolve().parents[1]
        cargo = tomllib.loads((root / "Cargo.toml").read_text())["package"]["include"]
        npm = json.loads((root / "npm/package.json").read_text())["files"]
        shipped = {"LICENSE", *release.DOCUMENTS}
        for name in release.DOCUMENTS:
            self.assertTrue((root / name).is_file(), name)
            self.assertTrue(any(fnmatch.fnmatch(name, pattern) for pattern in cargo), name)
            self.assertTrue(any(fnmatch.fnmatch(name, pattern) for pattern in npm), name)
            if not name.endswith(".md"):
                continue
            for target in re.findall(r"\]\(([^)]+)\)", (root / name).read_text()):
                if target.startswith(("https:", "http:", "#")):
                    continue
                linked = posixpath.normpath(posixpath.join(
                    posixpath.dirname(name), target.split("#")[0],
                ))
                self.assertIn(linked, shipped, f"{name}: {target}")


class ReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.addCleanup(patch.stopall)
        patch.object(release, "ROOT", self.root).start()
        patch.dict(release.os.environ, {"GITHUB_OUTPUT": ""}).start()
        (self.root / "npm/bin").mkdir(parents=True)
        (self.root / "Cargo.toml").write_text(
            '[package]\nname = "dsh-delegate-rs"\nversion = "0.1.0"\nrepository = "https://github.com/example/dsh-delegate-rs"\n'
        )
        (self.root / "Cargo.lock").write_text(
            'version = 4\n\n[[package]]\nname = "dsh-delegate-rs"\nversion = "0.1.0"\n'
            '\n[[package]]\nname = "dependency"\nversion = "0.1.0"\nsource = "registry+https://example.invalid/index"\n'
        )
        (self.root / ".gitignore").write_text("/target/\n/artifacts/\n/dist/\n")
        self.manifest = {
            "name": "dsh-delegate-rs", "version": "0.1.0",
            "repository": {"url": "git+https://github.com/example/dsh-delegate-rs.git"},
            "bin": {"dsh-delegate-rs": "bin/dsh-delegate-rs.cjs"},
            "files": ["bin", *release.DOCUMENTS, "LICENSE"],
            "optionalDependencies": release.platform_dependencies("0.1.0"),
        }
        (self.root / "npm/package.json").write_text(json.dumps(self.manifest))
        (self.root / "npm/bin/dsh-delegate-rs.cjs").write_text("#!/usr/bin/env node\n")
        for name in ("LICENSE", *release.DOCUMENTS):
            (self.root / name).parent.mkdir(parents=True, exist_ok=True)
            (self.root / name).write_text(name)
        self.git("init", "-b", "main")
        self.git("config", "user.name", "Release test")
        self.git("config", "user.email", "release@example.invalid")
        self.git("add", ".")
        self.git("commit", "-m", "fixture")

    def git(self, *args):
        return release.command("git", *args)

    def validate(self, tag="v0.1.0", repository="example/dsh-delegate-rs"):
        with contextlib.redirect_stdout(io.StringIO()):
            release.validate(tag, "main", repository)

    def test_annotated_tag_on_main_is_valid(self):
        self.git("tag", "-a", "v0.1.0", "-m", "release")
        self.validate()

    def test_tag_drives_all_versions_without_a_version_commit(self):
        cargo = self.root / "Cargo.toml"
        cargo.write_text(cargo.read_text() + '\n[package.metadata]\nversion = "9.9.9"\n')
        self.git("add", "Cargo.toml")
        self.git("commit", "-m", "package metadata fixture")
        self.git("tag", "-a", "v1.2.3", "-m", "release")
        head = self.git("rev-parse", "HEAD")
        release.prepare("v1.2.3")
        self.validate("v1.2.3")
        manifest = release.tomllib.loads(cargo.read_text())
        self.assertEqual(manifest["package"]["version"], "1.2.3")
        self.assertEqual(manifest["package"]["metadata"]["version"], "9.9.9")
        packages = release.tomllib.loads((self.root / "Cargo.lock").read_text())["package"]
        self.assertEqual([(p["name"], p["version"]) for p in packages],
                         [("dsh-delegate-rs", "1.2.3"), ("dependency", "0.1.0")])
        npm = json.loads((self.root / "npm/package.json").read_text())
        expected = self.manifest | {"version": "1.2.3", "optionalDependencies": release.platform_dependencies("1.2.3")}
        self.assertEqual(npm, expected)
        self.assertEqual(set(self.git("diff", "--name-only").splitlines()), set(release.VERSION_FILES))
        before = {name: (self.root / name).read_bytes() for name in release.VERSION_FILES}
        release.prepare("v1.2.3")
        self.assertEqual(before, {name: (self.root / name).read_bytes() for name in release.VERSION_FILES})
        self.assertEqual(self.git("rev-parse", "HEAD"), head)
        self.assertIn('version = "0.1.0"', self.git("show", "HEAD:Cargo.toml"))

    def test_prepare_rejects_invalid_tags_without_changing_files(self):
        files = ("Cargo.toml", "Cargo.lock", "npm/package.json")
        before = {name: (self.root / name).read_bytes() for name in files}
        for tag in ("0.2.0", "v0.2", "v00.2.0", "v0.2.0-rc.1", "v0.2.0+build"):
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                release.prepare(tag)
            self.assertEqual(before, {name: (self.root / name).read_bytes() for name in files})

    def test_prepare_and_crate_publishing_reject_unrelated_changes(self):
        uploader = Path(__file__).with_name("publish-crate.py")
        for name in ("README.md", "unexpected-source"):
            path = self.root / name
            original = path.read_bytes() if path.exists() else None
            path.write_text("unrelated change")
            with self.subTest(path=name), self.assertRaisesRegex(ValueError, "unrelated"):
                release.prepare("v0.2.0")
            with patch("subprocess.run", wraps=subprocess.run) as run:
                with self.assertRaisesRegex(ValueError, "unrelated"):
                    runpy.run_path(str(uploader))
                self.assertFalse(any(call.args[0][0] == "cargo" for call in run.call_args_list))
            if original is None:
                path.unlink()
            else:
                path.write_bytes(original)

    def test_prepare_rejects_missing_root_lock_entry_without_partial_updates(self):
        lock = self.root / "Cargo.lock"
        lock.write_text('version = 4\n\n[[package]]\nname = "other"\nversion = "0.1.0"\n')
        before = {name: (self.root / name).read_bytes() for name in ("Cargo.toml", "Cargo.lock", "npm/package.json")}
        with self.assertRaises(ValueError):
            release.prepare("v0.2.0")
        self.assertEqual(before, {name: (self.root / name).read_bytes() for name in before})

    def test_main_history_is_valid_even_after_branch_advances(self):
        self.git("tag", "v0.1.0")
        self.git("commit", "--allow-empty", "-m", "next")
        self.git("checkout", "v0.1.0")
        self.validate()

    def test_side_branch_tag_is_rejected(self):
        self.git("checkout", "-b", "side")
        self.git("commit", "--allow-empty", "-m", "side only")
        self.git("tag", "v0.1.0")
        with self.assertRaises(subprocess.CalledProcessError):
            self.validate()

    def test_bad_versions_and_wrong_repository_are_rejected(self):
        self.git("tag", "v0.1.0")
        for tag in ("v0.1", "v00.1.0", "v0.1.0-rc.1", "v0.1.1"):
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                self.validate(tag)
        with self.assertRaises(ValueError):
            self.validate(repository="other/dsh-delegate-rs")
        self.manifest["version"] = "0.1.1"
        (self.root / "npm/package.json").write_text(json.dumps(self.manifest))
        with self.assertRaises(ValueError):
            self.validate()

    def test_checkout_must_match_tag(self):
        self.git("tag", "v0.1.0")
        self.git("commit", "--allow-empty", "-m", "next")
        with self.assertRaises(ValueError):
            self.validate()

    def test_missing_or_mismatched_platform_dependency_is_rejected(self):
        self.git("tag", "v0.1.0")
        self.manifest["optionalDependencies"]["dsh-delegate-rs-linux-arm64"] = "0.0.9"
        (self.root / "npm/package.json").write_text(json.dumps(self.manifest))
        with self.assertRaisesRegex(ValueError, "platform dependencies"):
            self.validate()
        self.manifest["optionalDependencies"].pop("dsh-delegate-rs-linux-arm64")
        (self.root / "npm/package.json").write_text(json.dumps(self.manifest))
        with self.assertRaisesRegex(ValueError, "platform dependencies"):
            release.assemble(self.make_artifacts(), self.root / "dist", "example/dsh-delegate-rs")

    def make_artifacts(self, version="0.1.0"):
        artifacts = self.root / "artifacts"
        for target, platform in release.TARGETS.items():
            name = "dsh-delegate-rs.exe" if platform.startswith("win32") else "dsh-delegate-rs"
            binary = self.root / "target" / target / "release" / name
            binary.parent.mkdir(parents=True)
            binary.write_bytes(f"binary:{platform}".encode())
            binary.chmod(0o755)
            release.archive(target, version, artifacts / target)
        return artifacts

    def test_tag_derived_version_reaches_all_npm_archives(self):
        self.git("tag", "v1.2.3")
        release.prepare("v1.2.3")
        self.validate("v1.2.3")
        output = self.root / "dist"
        release.assemble(self.make_artifacts("1.2.3"), output, "example/dsh-delegate-rs")
        for name in ("dsh-delegate-rs", *release.platform_dependencies("1.2.3")):
            with self.subTest(package=name), tarfile.open(output / "assets" / f"{name}-1.2.3.tgz") as bundle:
                manifest = json.load(bundle.extractfile("package/package.json"))
                self.assertEqual(manifest["version"], "1.2.3")
                if name == "dsh-delegate-rs":
                    self.assertEqual(manifest["optionalDependencies"], release.platform_dependencies("1.2.3"))

    def test_missing_matrix_member_prevents_packaging(self):
        artifacts = self.make_artifacts()
        (artifacts / "aarch64-apple-darwin/dsh-delegate-rs").unlink()
        output = self.root / "dist"
        with self.assertRaises(ValueError):
            release.assemble(artifacts, output, "example/dsh-delegate-rs")
        self.assertFalse(output.exists())

    def test_archives_npm_contents_and_checksums(self):
        artifacts = self.make_artifacts()
        output = self.root / "dist"
        release.assemble(artifacts, output, "example/dsh-delegate-rs")
        assets = output / "assets"
        with tarfile.open(assets / "dsh-delegate-rs-0.1.0.tgz") as bundle:
            members = {item.name: item for item in bundle.getmembers()}
            self.assertEqual(set(members), {
                "package/package.json", "package/bin/dsh-delegate-rs.cjs", "package/LICENSE",
                "package/README.md", "package/README.en.md", "package/INSTALL.md", "package/INSTALL.en.md",
                "package/docs/dsh-contract.md",
                "package/docs/validation.md", "package/examples/commandcode.yml",
            })
            manifest = json.load(bundle.extractfile("package/package.json"))
            self.assertEqual(manifest["repository"]["url"], "git+https://github.com/example/dsh-delegate-rs.git")
            self.assertEqual(manifest["optionalDependencies"], release.platform_dependencies("0.1.0"))
        for platform in release.TARGETS.values():
            with tarfile.open(assets / f"dsh-delegate-rs-{platform}-0.1.0.tgz") as bundle:
                members = {item.name: item for item in bundle.getmembers()}
                name = "dsh-delegate-rs.exe" if platform.startswith("win32") else "dsh-delegate-rs"
                member = members[f"package/bin/{name}"]
                self.assertEqual(bundle.extractfile(member).read(), f"binary:{platform}".encode())
                if not platform.startswith("win32"):
                    self.assertTrue(member.mode & 0o111)
                self.assertEqual(set(members), {
                    "package/package.json", f"package/bin/{name}", "package/README.md", "package/LICENSE",
                })
                manifest = json.load(bundle.extractfile("package/package.json"))
                self.assertEqual(manifest["name"], f"dsh-delegate-rs-{platform}")
                self.assertEqual(manifest["version"], "0.1.0")
                system, cpu = platform.split("-")
                self.assertEqual(manifest["os"], [system])
                self.assertEqual(manifest["cpu"], [cpu])
        for target, platform in release.TARGETS.items():
            stem = f"dsh-delegate-rs-v0.1.0-{target}"
            if platform.startswith("win32"):
                with zipfile.ZipFile(assets / f"{stem}.zip") as bundle:
                    paths = set(bundle.namelist())
            else:
                with tarfile.open(assets / f"{stem}.tar.gz") as bundle:
                    paths = set(bundle.getnames())
            for name in release.DOCUMENTS:
                self.assertIn(f"{stem}/{name}", paths)
        for line in (assets / "SHA256SUMS").read_text().splitlines():
            digest, name = line.split("  ")
            self.assertEqual(digest, hashlib.sha256((assets / name).read_bytes()).hexdigest())
        self.assertEqual(len(list(assets.iterdir())), 14)
        # Replace only registry locations for an offline installation test;
        # all native tarballs retain their generated os/cpu restrictions.
        package = output / "npm"
        manifest = json.loads((package / "package.json").read_text())
        manifest["optionalDependencies"] = {
            name: (assets / f"{name}-0.1.0.tgz").as_uri()
            for name in release.platform_dependencies("0.1.0")
        }
        (package / "package.json").write_text(json.dumps(manifest))
        local = output / "local"
        local.mkdir()
        release.command("npm", "pack", "--silent", "--ignore-scripts", "--pack-destination", str(local), cwd=package)
        for platform in release.TARGETS.values():
            with self.subTest(install=platform):
                system, cpu = platform.split("-")
                prefix = output / f"install-{platform}"
                release.command(
                    "npm", "install", "--ignore-scripts", "--offline", "--no-audit", "--no-fund",
                    "--package-lock=false", f"--os={system}", f"--cpu={cpu}", "--prefix", str(prefix),
                    str(local / "dsh-delegate-rs-0.1.0.tgz"),
                )
                installed = {p.name for p in (prefix / "node_modules").glob("dsh-delegate-rs-*")}
                self.assertEqual(installed, {f"dsh-delegate-rs-{platform}"})

    def test_public_release_reuse_verifies_integrity_without_replacing_archives(self):
        output = self.root / "dist"
        release.assemble(self.make_artifacts(), output, "example/dsh-delegate-rs")
        assets = output / "assets"
        published = self.root / "published"
        release.shutil.copytree(assets, published)
        # Repacking archive headers must not replace already-published bytes.
        archive = next(item for item in assets.iterdir() if item.name.endswith(".tar.gz"))
        archive.write_bytes(b"repacked")
        release.reuse_assets(published, assets)
        self.assertEqual(archive.read_bytes(), (published / archive.name).read_bytes())
        (published / "dsh-delegate-rs-linux-arm64-0.1.0.tgz").write_bytes(b"different valid publication")
        checksums = [
            f"{hashlib.sha256(item.read_bytes()).hexdigest()}  {item.name}\n"
            for item in sorted(published.iterdir()) if item.name != "SHA256SUMS"
        ]
        (published / "SHA256SUMS").write_text("".join(checksums))
        with self.assertRaisesRegex(ValueError, "npm payload differs"):
            release.reuse_assets(published, assets)

    @patch.object(release, "check_version_changes")
    def test_crate_publication_checks_existing_and_ambiguous_uploads(self, check_changes):
        archive = self.root / "target/package/dsh-delegate-rs-0.1.0.crate"
        archive.parent.mkdir(parents=True)
        archive.write_bytes(b"crate fixture")
        checksum = hashlib.sha256(archive.read_bytes()).hexdigest()
        uploader = Path(__file__).with_name("publish-crate.py")
        match = json.dumps({"version": {"checksum": checksum}}).encode()
        missing = urllib.error.HTTPError("test", 404, "missing", {}, None)
        for queries, status, should_publish in [([match], 0, False), ([missing], 0, True), ([missing, match], 1, True)]:
            responses = [io.BytesIO(item) if isinstance(item, bytes) else item for item in queries]
            with patch("subprocess.run") as run, patch("urllib.request.urlopen", side_effect=responses), patch("time.sleep"):
                run.return_value.returncode = status
                runpy.run_path(str(uploader))
                self.assertEqual(any(call.args[0][1] == "publish" for call in run.call_args_list), should_publish)
                self.assertTrue(all("--allow-dirty" in call.args[0] for call in run.call_args_list))
        mismatch = io.BytesIO(b'{"version":{"checksum":"different"}}')
        with patch("subprocess.run") as run, patch("urllib.request.urlopen", return_value=mismatch):
            with self.assertRaises(ValueError):
                runpy.run_path(str(uploader))
            self.assertEqual(run.call_count, 1)


if __name__ == "__main__":
    unittest.main()
