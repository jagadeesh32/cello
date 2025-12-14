# Publishing Guide

This guide explains how to publish Cello to GitHub and PyPI.

## Prerequisites

1. **GitHub Account** with a repository for cello
2. **PyPI Account** at https://pypi.org
3. **API Tokens** for PyPI authentication

## GitHub Setup

### 1. Initialize Git Repository

```bash
cd cello
git init
git add .
git commit -m "Initial commit: Cello v2.0.0"
```

### 2. Create GitHub Repository

1. Go to https://github.com/new
2. Create a new repository named `cello`
3. **Don't** initialize with README (we already have one)

### 3. Push to GitHub

```bash
git remote add origin https://github.com/YOUR_USERNAME/cello.git
git branch -M main
git push -u origin main
```

## PyPI Setup

### 1. Create PyPI API Token

1. Go to https://pypi.org/manage/account/
2. Scroll to "API tokens"
3. Click "Add API token"
4. Name: `cello-github-actions`
5. Scope: `Entire account` (or project-specific after first upload)
6. Copy the token (starts with `pypi-`)

### 2. Add Token to GitHub Secrets

1. Go to your GitHub repo → Settings → Secrets and variables → Actions
2. Click "New repository secret"
3. Name: `PYPI_API_TOKEN`
4. Value: Paste your PyPI token
5. Click "Add secret"

### 3. (Optional) Test PyPI Token

For testing, also create a Test PyPI token:
1. Go to https://test.pypi.org/manage/account/
2. Create token named `cello-github-actions`
3. Add to GitHub as `TEST_PYPI_API_TOKEN`

## Publishing Workflow

### Automated Publishing (Recommended)

The GitHub Actions workflow (`.github/workflows/publish.yml`) automatically publishes when you create a release:

1. **Update version** in `Cargo.toml` and `pyproject.toml`:
   ```toml
   # Cargo.toml
   version = "2.0.0"
   
   # pyproject.toml
   version = "2.0.0"
   ```

2. **Commit and push**:
   ```bash
   git add Cargo.toml pyproject.toml
   git commit -m "Bump version to 2.0.0"
   git push
   ```

3. **Create a GitHub Release**:
   - Go to your repo → Releases → Draft a new release
   - Tag: `v2.0.0` (create new tag)
   - Title: `Cello v2.0.0`
   - Description: Release notes
   - Click "Publish release"

4. **GitHub Actions** will automatically:
   - Build wheels for Linux, macOS, Windows (x86_64 and aarch64)
   - Build source distribution
   - Upload all to PyPI

### Manual Publishing

If you prefer to publish manually:

```bash
# Install maturin
pip install maturin twine

# Build wheels
maturin build --release

# Build source distribution
maturin sdist

# Upload to PyPI
twine upload target/wheels/*.whl target/wheels/*.tar.gz
```

## Version Numbering

Follow semantic versioning (SemVer):
- **MAJOR.MINOR.PATCH** (e.g., `2.0.0`)
- **MAJOR**: Breaking changes
- **MINOR**: New features (backward compatible)
- **PATCH**: Bug fixes (backward compatible)

## Post-Publishing

After publishing, users can install with:

```bash
pip install cello
```

## Troubleshooting

### "Project name already exists"
- PyPI names are globally unique
- Try: `cello-framework`, `py-cello`, or similar

### "Invalid API token"
- Ensure token is correctly copied
- Check token scope includes your project
- Regenerate if needed

### Build fails on CI
- Check Rust version compatibility
- Verify maturin version matches locally
- Review failed action logs

## Files Structure for Publishing

```
cello/
├── .github/
│   └── workflows/
│       ├── ci.yml          # CI: lint, test, build
│       └── publish.yml     # Publish to PyPI
├── src/                    # Rust source
├── python/cello/          # Python package
├── tests/                  # Python tests
├── Cargo.toml              # Rust config (version here!)
├── pyproject.toml          # Python config (version here!)
├── README.md               # PyPI description
└── LICENSE                 # MIT license
```
