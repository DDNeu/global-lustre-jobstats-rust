#!/bin/bash
#
# release.sh - Automated release script for glljobstat
#
# Usage:
#   ./release.sh <version>        # e.g., ./release.sh 1.1.0
#   ./release.sh --bump patch     # Auto-bump patch version
#   ./release.sh --bump minor     # Auto-bump minor version
#   ./release.sh --bump major     # Auto-bump major version
#
# Options:
#   --dry-run    Show what would be done without making changes
#   --no-push    Create tag but don't push or create GitHub release
#   --force      Skip confirmation prompts
#

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
BINARY_NAME="glljobstat"
RELEASE_DIR="release-bundles"
ASSETS=(
    "target/release/${BINARY_NAME}"
    "completions/glljobstat.bash"
    "README.md"
)

# Flags
DRY_RUN=false
NO_PUSH=false
FORCE=false
BUMP_TYPE=""
VERSION=""

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --dry-run) DRY_RUN=true; shift ;;
        --no-push) NO_PUSH=true; shift ;;
        --force) FORCE=true; shift ;;
        --bump) BUMP_TYPE="$2"; shift 2 ;;
        -h|--help)
            echo "Usage: $0 [OPTIONS] <version>"
            echo ""
            echo "Arguments:"
            echo "  version          Version to release (e.g., 1.1.0)"
            echo ""
            echo "Options:"
            echo "  --bump TYPE      Auto-bump version (major|minor|patch)"
            echo "  --dry-run        Show what would be done without changes"
            echo "  --no-push        Create tag locally but don't push"
            echo "  --force          Skip confirmation prompts"
            echo "  -h, --help       Show this help"
            exit 0
            ;;
        *) VERSION="$1"; shift ;;
    esac
done

log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[SUCCESS]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
    if [[ -n "$2" ]]; then
        echo -e "${RED}        Details:${NC}"
        echo "$2" | sed 's/^/        /'
    fi
    exit 1
}

get_current_version() {
    grep '^version = ' Cargo.toml | sed 's/version = "\(.*\)"/\1/'
}

bump_version() {
    local current="$1" type="$2"
    IFS='.' read -r major minor patch <<< "$current"
    case $type in
        major) echo "$((major + 1)).0.0" ;;
        minor) echo "${major}.$((minor + 1)).0" ;;
        patch) echo "${major}.${minor}.$((patch + 1))" ;;
        *) log_error "Invalid bump type: $type (use major|minor|patch)" ;;
    esac
}

validate_version() {
    local version="$1"
    [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || \
        log_error "Invalid version format: $version (expected X.Y.Z)"
}

check_prerequisites() {
    log_info "Checking prerequisites..."

    # Check required commands
    for cmd in git cargo gh; do
        if ! command -v $cmd &> /dev/null; then
            log_error "$cmd is required but not installed" \
                "Install $cmd and ensure it's in your PATH"
        fi
    done

    # Check we're in project root
    if [[ ! -f "Cargo.toml" ]]; then
        log_error "Must be run from project root (Cargo.toml not found)" \
            "Current directory: $(pwd)"
    fi

    # Check for uncommitted changes
    local dirty_files=$(git status --porcelain)
    if [[ -n "$dirty_files" ]]; then
        log_error "Working directory is not clean. Commit or stash changes first." \
            "$dirty_files"
    fi

    # Check branch
    local branch=$(git branch --show-current)
    if [[ "$branch" != "main" && "$branch" != "master" ]]; then
        log_warn "Not on main/master branch (currently on: $branch)"
        if [[ "$FORCE" != "true" ]]; then
            read -p "Continue anyway? [y/N] " -n 1 -r; echo
            [[ $REPLY =~ ^[Yy]$ ]] || exit 1
        fi
    fi
    log_success "Prerequisites check passed"
}

check_tag_exists() {
    local tag="v$1"
    if git rev-parse "$tag" >/dev/null 2>&1; then
        local tag_commit=$(git rev-parse "$tag")
        local tag_date=$(git log -1 --format=%ci "$tag" 2>/dev/null || echo "unknown")
        log_error "Tag $tag already exists" \
            "Commit: $tag_commit
Created: $tag_date
Use a different version number or delete the existing tag with:
  git tag -d $tag && git push origin :refs/tags/$tag"
    fi
}

update_cargo_version() {
    local version="$1"
    log_info "Updating Cargo.toml version to $version..."
    if [[ "$DRY_RUN" == "true" ]]; then
        log_info "[DRY-RUN] Would update Cargo.toml version to $version"; return
    fi
    sed -i "s/^version = \".*\"/version = \"$version\"/" Cargo.toml
    log_success "Updated Cargo.toml"
}

build_release() {
    log_info "Building release binary..."
    if [[ "$DRY_RUN" == "true" ]]; then
        log_info "[DRY-RUN] Would run: cargo build --release -j8"; return
    fi
    cargo build --release -j8
    log_success "Build completed"
}

run_tests() {
    log_info "Running tests..."
    if [[ "$DRY_RUN" == "true" ]]; then
        log_info "[DRY-RUN] Would run: cargo test"; return
    fi
    cargo test
    log_success "Tests passed"
}

verify_assets() {
    log_info "Verifying release assets..."
    for asset in "${ASSETS[@]}"; do
        if [[ ! -f "$asset" ]]; then
            log_error "Required asset not found: $asset" \
                "Expected at: $(pwd)/$asset
Run 'cargo build --release' to create the binary, or check that all required files exist."
        fi
        log_info "  ✓ $asset"
    done
    log_success "All assets verified"
}

# Get the archive filename for a version
get_archive_name() {
    local version="$1"
    echo "${BINARY_NAME}-${version}-linux-x86_64.tar.gz"
}

# Get the full archive path (in release-bundles directory)
get_archive_path() {
    local version="$1"
    echo "${RELEASE_DIR}/$(get_archive_name "$version")"
}

create_archive() {
    local version="$1"
    local archive_name=$(get_archive_name "$version")
    local archive_path=$(get_archive_path "$version")
    # Directory name matches archive name without .tar.gz extension
    local staging_dir="${archive_name%.tar.gz}"

    log_info "Creating release archive: $archive_path"
    log_info "  Extracts to: $staging_dir/"

    if [[ "$DRY_RUN" == "true" ]]; then
        log_info "[DRY-RUN] Would create archive: $archive_path"
        log_info "[DRY-RUN] Extracts to: $staging_dir/"
        log_info "[DRY-RUN] Contents:"
        for asset in "${ASSETS[@]}"; do
            log_info "  - $(basename "$asset")"
        done
        return
    fi

    # Create release-bundles directory if it doesn't exist
    mkdir -p "$RELEASE_DIR"

    # Create staging directory
    rm -rf "$staging_dir"
    mkdir -p "$staging_dir"

    # Copy assets to staging directory
    for asset in "${ASSETS[@]}"; do
        cp "$asset" "$staging_dir/"
    done

    # Create tar.gz archive in release-bundles directory
    tar -czvf "$archive_path" "$staging_dir"

    # Cleanup staging directory
    rm -rf "$staging_dir"

    log_success "Created archive: $archive_path"
}

create_tag() {
    local version="$1"
    local tag="v$version"
    local archive_path=$(get_archive_path "$version")
    log_info "Creating git commit and tag $tag..."
    if [[ "$DRY_RUN" == "true" ]]; then
        log_info "[DRY-RUN] Would commit Cargo.toml, Cargo.lock, and $archive_path"
        log_info "[DRY-RUN] Would create tag $tag"
        return
    fi
    git add Cargo.toml Cargo.lock "$archive_path"
    git commit -m "Release $tag"
    git tag -a "$tag" -m "Release $tag"
    log_success "Created tag $tag"
}

push_to_remote() {
    local tag="v$1"
    if [[ "$NO_PUSH" == "true" ]]; then
        log_warn "Skipping push (--no-push specified)"; return
    fi
    log_info "Pushing to remote..."
    if [[ "$DRY_RUN" == "true" ]]; then
        log_info "[DRY-RUN] Would push commits and tag $tag"; return
    fi
    git push origin HEAD
    git push origin "$tag"
    log_success "Pushed to remote"
}

create_github_release() {
    local version="$1"
    local tag="v$version"
    local archive_path=$(get_archive_path "$version")

    if [[ "$NO_PUSH" == "true" ]]; then
        log_warn "Skipping GitHub release (--no-push specified)"; return
    fi
    log_info "Creating GitHub release $tag..."

    # Generate release notes from commits since last tag
    local last_tag=$(git describe --tags --abbrev=0 HEAD^ 2>/dev/null || echo "")
    local release_notes=""
    if [[ -n "$last_tag" ]]; then
        release_notes=$(git log "$last_tag"..HEAD --pretty=format:"- %s" --no-merges)
    else
        release_notes="Initial release"
    fi

    if [[ "$DRY_RUN" == "true" ]]; then
        log_info "[DRY-RUN] Would create GitHub release with:"
        log_info "  Tag: $tag"
        log_info "  Archive: $archive_path"
        log_info "  Release notes:"
        echo "$release_notes"
        return
    fi

    # Verify archive exists
    if [[ ! -f "$archive_path" ]]; then
        log_error "Archive not found: $archive_path" \
            "Expected at: $(pwd)/$archive_path
The create_archive step may have failed. Check for errors above."
    fi

    gh release create "$tag" \
        --title "Release $tag" \
        --notes "$release_notes" \
        --latest \
        "$archive_path"

    log_success "GitHub release created: $tag"
}

show_summary() {
    local version="$1"
    local archive_path=$(get_archive_path "$version")
    echo ""
    echo "=========================================="
    echo "  Release Summary"
    echo "=========================================="
    echo "  Version:  $version"
    echo "  Tag:      v$version"
    echo "  Archive:  $archive_path"
    echo "  Assets:"
    for asset in "${ASSETS[@]}"; do
        echo "    - $asset"
    done
    echo "=========================================="
    echo ""
}

main() {
    echo ""
    echo "=========================================="
    echo "  glljobstat Release Script"
    echo "=========================================="
    echo ""
    
    # Determine version
    local current_version=$(get_current_version)
    log_info "Current version: $current_version"
    
    if [[ -n "$BUMP_TYPE" ]]; then
        VERSION=$(bump_version "$current_version" "$BUMP_TYPE")
        log_info "Bumping $BUMP_TYPE version: $current_version -> $VERSION"
    fi
    
    if [[ -z "$VERSION" ]]; then
        log_error "No version specified. Use: $0 <version> or $0 --bump <type>"
    fi
    
    validate_version "$VERSION"
    
    # Show what we're going to do
    show_summary "$VERSION"
    
    # Confirmation
    if [[ "$FORCE" != "true" && "$DRY_RUN" != "true" ]]; then
        read -p "Proceed with release? [y/N] " -n 1 -r
        echo
        [[ $REPLY =~ ^[Yy]$ ]] || exit 1
    fi
    
    # Execute release steps
    check_prerequisites
    check_tag_exists "$VERSION"
    update_cargo_version "$VERSION"
    build_release
    run_tests
    verify_assets
    create_archive "$VERSION"
    create_tag "$VERSION"
    push_to_remote "$VERSION"
    create_github_release "$VERSION"
    
    echo ""
    log_success "Release $VERSION completed successfully!"
    echo ""
}

main "$@"
