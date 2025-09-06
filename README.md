# Super Simple CDN

Things this should be able to do:

- [x] Serve static files (obviously)
    - [x] Automatically compress files (gzip, brotli)
    - [x] Cache files in memory for faster access
    - [x] Handle large files efficiently (streaming)
    - [ ] CORS rules
    - [ ] Encrypt files at rest
- Two different access modes
    - [x] API access (cdn.example.com/`{file}`)
    - [ ] Web access (files.example.com/`{file}`)
- API for file management
    - [ ] Simple JWT authentication
    - [ ] `POST /{file}` to upload files
    - [ ] `PUT /{file}` to update files
    - [ ] `DELETE /{file}` to delete files
