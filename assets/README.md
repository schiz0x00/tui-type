# Asset provenance

`words.txt` is a project-curated collection of ordinary English words. Duplicate entries are removed during loading as an additional safeguard.

`quotes.json` contains original typing-practice sentences written for this project. They are prompts rather than attributed quotations; the historical filename is retained for format compatibility.

Both files are distributed under the repository's MIT license. No third-party text corpus or bundled binary asset is used at runtime. The files are embedded in the executable at compile time; pass `--wordlist PATH` to use a runtime-editable word list.
