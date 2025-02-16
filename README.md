# Release Sanity Checker

This tool allows you to check for differences in endpoint responses before and after a change.

## How to run

- **Download binary**  
Download your preferred version from [releases](https://github.com/drew458/release-sanity-checker/releases) and just run `./release-sanity-checker <FILENAME>`.

- **Container**  
Assuming you have Podman or Docker installed on your machine:
    - `podman build -t release-sanity-checker .`
    - `podman run -v "<FILENAME>:/app/config.json" -v release-sanity-checker /app/config.json`  
    
    If you want to run it against a directory, use:  
    - `podman run -v "<FILENAME>:/app/examples" -v release-sanity-checker --directory /app/examples`  
