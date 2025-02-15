# Response Checker

This tool allows you to check for differences in endpoint responses before and after a change.

## How to run

- **Short option**  
If you are on MacOS (AArch64), just run `./response_checker <FILENAME>`.

- **Long option**  
Assuming you have Podman or Docker installed on your machine:
    - `podman build -t response-checker .`
    - `podman run -v "<FILENAME>:/app/config.json" -v response-checker /app/config.json`  
    
    If you want to run it against a directory, use:  
    - `podman run -v "<FILENAME>:/app/examples" -v response-checker --directory /app/examples`  
