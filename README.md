# Super Simple CDN

Things this should be able to do:

- [x] Serve static files (obviously)
    - [x] Automatically compress files (gzip, brotli)
    - [x] Caching
        - [x] Cache-Control headers
        - [x] Server-side caching (in-memory or on-disk if remote is added)
        - [ ] ETag headers
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
