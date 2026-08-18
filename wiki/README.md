# Wiki staging folder

This folder holds the GitHub wiki content (bilingual: English pages and
`-CN` Chinese pages). The GitHub wiki is a **separate git repository**; the
files here are the staging copy. They are excluded from the published crate
(`exclude = ["wiki/"]` in `Cargo.toml`).

## Pushing to the GitHub wiki

The wiki lives at `https://github.com/blueokanna/Tzcraft.wiki` (clone URL:
`https://github.com/blueokanna/Tzcraft.wiki.git`). The home page must be
named `Home.md`; all other pages are referenced by their file name
(without `.md`).

From this repository, push the staging copy with:

```sh
git clone https://github.com/blueokanna/Tzcraft.wiki.git /tmp/tzcraft-wiki
cp wiki/*.md /tmp/tzcraft-wiki/
cd /tmp/tzcraft-wiki
git add -A
git commit -m "Publish bilingual wiki"
git push
```

Or use the wiki URL with a remote added from inside this repo:

```sh
git remote add wiki https://github.com/blueokanna/Tzcraft.wiki.git
git subtree push --prefix=wiki wiki master
```

After pushing, verify:

- `Home.md` renders as the wiki home page;
- the sidebar lists the six pages (12 files total, `Home`/`Home-CN` and the
  `-CN` counterparts);
- internal wiki links (`[Design and Architecture](Design-and-Architecture)`)
  resolve.

## Page index

| English | Chinese |
| --- | --- |
| `Home.md` | `Home-CN.md` |
| `Design-and-Architecture.md` | `Design-and-Architecture-CN.md` |
| `no_std-and-Features.md` | `no_std-and-Features-CN.md` |
| `Y2038.md` | `Y2038-CN.md` |
| `Migration-Guide.md` | `Migration-Guide-CN.md` |
| `Safety-and-Testing.md` | `Safety-and-Testing-CN.md` |
| `Publishing.md` | `Publishing-CN.md` |
