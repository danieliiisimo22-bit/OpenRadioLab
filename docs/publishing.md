# Publishing the source

The source tree is prepared for a repository named `open-radio-lab`. The name is a project choice, not a claim of exclusive naming rights or universal device support.

Before creating a remote, run the README checks and review the actual files selected for commit. The repository includes a GPL license, contribution guide, compatibility documentation, issue/PR templates, source-language inventory and a Windows CI workflow.

```sh
git init -b main
git add .
git diff --cached --stat
git diff --cached --check
git commit -m "Initial Open Radio Lab source release"
```

Create an empty repository in the intended GitHub account, then use the remote URL supplied by that account:

```sh
git remote add origin REMOTE_URL
git push -u origin main
```

`REMOTE_URL` is a placeholder. No account, credentials or remote repository is embedded in the source, and preparation does not mean the source has already been published.

Use release assets for portable binaries and include matching source and dependency notices. Keep device backups, private channel data and serial reports local. The standard source guard is a useful boundary check, not a comprehensive credential or license audit.

