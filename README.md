# Polymath : High performance chunking and embedding

A high-performance web service that processes search queries, retrieves relevant content, and performs semantic chunking and embedding operations.

## 🌟 Features

- Search query processing using SearxNG integration
- Content chunking and embedding generation
- Vector similarity search using USearch
- OpenAPI documentation with multiple UI options (Swagger, Redoc, RapiDoc)
- Asynchronous processing of multiple URLs
- Environment-based configuration
- Comprehensive logging system

## 🚀 Getting Started

### Prerequisites

- Rust (latest stable version)
- SearxNG instance
- Content processor service
- Environment variables configuration

### Installation

1. Clone the repository:
```bash
git clone https://github.com/sokratis-xyz/polymath.git
cd polymath
```

2. Create a `.env` file in the project root with the following variables:
```env
SEARXNG_URL=http://your-searxng-instance/search
PROCESSOR_URL=http://your-processor-service/process
SERVER_HOST=127.0.0.1
SERVER_PORT=8080
```

3. Build the project:
```bash
cargo build --release
```

### Running the Service

```bash
cargo run --release
```

The service will be available at `http://localhost:8080` (or your configured host/port).

## 🔍 API Endpoints

### Search Endpoint

```
GET /v1/search?q={query}
```

Parameters:
- `q` (required): The search query string

Response: Returns a map of processed content chunks with their associated URLs and text.

## 📚 Documentation

The API documentation is available at the following endpoints:

- Swagger UI: `/swagger`
- Redoc: `/redoc`
- RapiDoc: `/rapidoc`
- Scalar: `/scalar`
- OpenAPI JSON: `/openapi.json`

## 🛠 Technical Details

### Architecture

The service is built using:
- Actix-web for the HTTP server
- USearch for vector similarity search
- Serde for serialization/deserialization
- Tokio for async runtime
- Reqwest for HTTP client operations

### Key Components

1. **Search Processing**: Handles search queries through SearxNG integration
2. **Content Processing**: Chunks content and generates embeddings
3. **Vector Indexing**: Manages similarity search using USearch
4. **Concurrent Processing**: Processes multiple URLs simultaneously using Tokio

### Configuration

The service uses the following configuration options for content processing:
- Chunking size: 100 words
- Embedding model: AllMiniLML6V2
- Vector dimensions: 384
- Metric: Inner Product (IP)

## 🤝 Contributing

Contributions are welcome! Please feel free to submit pull requests.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add some amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## 📝 License

This project is licensed under the GNU General Public License v3.0 (GPL-3.0) - see below for a summary:

GNU General Public License v3.0 (GPL-3.0)

Permissions:
- Commercial use
- Distribution
- Modification
- Patent use
- Private use

Conditions:
- Disclose source
- License and copyright notice
- Same license
- State changes

Limitations:
- Liability
- Warranty

For the full license text, see [LICENSE](LICENSE) or visit https://www.gnu.org/licenses/gpl-3.0.en.html

## 📧 Contact

support@sokratis.xyz
