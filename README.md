# Response Checker

This tool allows you to check for differences in endpoint responses before and after a change.

## How to run

- **Download binary**  
Download your preferred version from [releases](https://github.com/drew458/release-sanity-checker/releases) and just run `./response_checker <FILENAME>`.

- **Container**  
Assuming you have Podman or Docker installed on your machine:
    - `podman build -t response-checker .`
    - `podman run -v "<FILENAME>:/app/config.json" -v response-checker /app/config.json`  
    
    If you want to run it against a directory, use:  
    - `podman run -v "<FILENAME>:/app/examples" -v response-checker --directory /app/examples`  
