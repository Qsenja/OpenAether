#!/bin/bash

# Unified Package Manager for OpenEtude
# Priority: pacman (official) -> yay (AUR) -> npm (node)

ACTION=$1
shift
PACKAGES=("$@")

if [[ -z "$ACTION" ]]; then
    echo "Usage: $0 <install|remove|reinstall|search|check-installed|cleanup-orphans|clean-cache> [package1] ..."
    exit 1
fi

do_install() {
    local PKG=$1
    echo "--- Installing $PKG ---"
    if pacman -Si "$PKG" > /dev/null 2>&1; then
        echo "Installing $PKG via pacman..."
        pkexec pacman -S --noconfirm "$PKG"
    elif yay -Si "$PKG" > /dev/null 2>&1; then
        echo "Installing $PKG via yay (AUR)..."
        yay -S --noconfirm --answerclean None --answerdiff None --answeredit None --sudo pkexec "$PKG"
    else
        echo "Trying npm global install for $PKG..."
        npm install -g "$PKG"
    fi

    if [ $? -eq 0 ]; then
        echo "SUCCESS: $PKG installed successfully."
    else
        echo "FAILURE: Could not install $PKG."
        return 1
    fi
}

do_remove() {
    local PKG=$1
    echo "--- Removing $PKG ---"
    if pacman -Qi "$PKG" > /dev/null 2>&1; then
        echo "Removing $PKG via pacman..."
        pkexec pacman -Rns --noconfirm "$PKG"
    elif yay -Qi "$PKG" > /dev/null 2>&1; then
        echo "Removing $PKG via yay..."
        yay -Rns --noconfirm --answerclean None --answerdiff None --answeredit None --sudo pkexec "$PKG"
    elif npm list -g "$PKG" > /dev/null 2>&1; then
        echo "Removing $PKG via npm..."
        npm uninstall -g "$PKG"
    else
        echo "error: target not found: $PKG"
        return 1
    fi
    
    if [ $? -eq 0 ]; then
        echo "SUCCESS: $PKG removed successfully."
    else
        echo "FAILURE: Could not remove $PKG."
        return 1
    fi
}

do_check() {
    local PKG=$1
    # 1. Direct check
    if pacman -Qq "$PKG" > /dev/null 2>&1; then
        echo "FOUND_SYSTEM_PACKAGE: $PKG"
        pacman -Qi "$PKG" | grep -E "^Version|^Description"
        return 0
    fi

    # 2. Check if it's a binary name, then find owner
    local BIN_PATH=$(which "$PKG" 2>/dev/null)
    if [ -n "$BIN_PATH" ]; then
        local OWNER=$(pacman -Qo "$BIN_PATH" 2>/dev/null | awk '{print $5}')
        if [ -n "$OWNER" ]; then
            echo "FOUND_BINARY: $BIN_PATH (Owned by package: $OWNER)"
            return 0
        else
            echo "FOUND_UNOWNED_BINARY: $BIN_PATH"
            return 0
        fi
    fi

    # 3. Check AUR
    if yay -Qq "$PKG" > /dev/null 2>&1; then
        echo "FOUND_AUR_PACKAGE: $PKG"
        yay -Qi "$PKG" | grep -E "^Version|^Description"
        return 0
    fi

    echo "NOT_FOUND: $PKG"
    return 1
}

do_search() {
    local QUERY=$1
    echo "SEARCH_RESULTS_FOR: $QUERY"
    echo "--- Official Repos (Top 10) ---"
    pacman -Ss "$QUERY" | grep -i --color=never "$QUERY" | head -n 10
    echo ""
    echo "--- AUR (Top 10) ---"
    yay -Ss "$QUERY" | grep -i --color=never "$QUERY" | head -n 10
}

do_purge_data() {
    local PKG=$1
    echo "--- Purging User Data for $PKG ---"
    local TARGETS=(
        "$HOME/.config/$PKG"
        "$HOME/.local/share/$PKG"
        "$HOME/.cache/$PKG"
    )
    
    for T in "${TARGETS[@]}"; do
        if [ -d "$T" ]; then
            echo "Removing: $T"
            rm -rf "$T"
        fi
    done
    echo "SUCCESS: Data purge attempt completed for $PKG."
}

EXIT_CODE=0

for PKG in "${PACKAGES[@]}"; do
    case $ACTION in
        "install")
            do_install "$PKG" || EXIT_CODE=1
            ;;
        "remove")
            do_remove "$PKG" || EXIT_CODE=1
            ;;
        "reinstall")
            echo "--- Reinstalling $PKG ---"
            do_remove "$PKG" || true
            do_install "$PKG" || EXIT_CODE=1
            ;;
        "check-installed")
            do_check "$PKG" || EXIT_CODE=1
            ;;
        "search")
            do_search "$PKG"
            ;;
        "purge-data")
            do_purge_data "$PKG"
            ;;
        "cleanup-orphans")
            echo "--- Cleaning up orphaned packages ---"
            ORPHANS=$(pacman -Qtdq)
            if [ -n "$ORPHANS" ]; then
                echo "Found orphans: $ORPHANS"
                pkexec pacman -Rns --noconfirm $ORPHANS
            else
                echo "No orphaned packages found."
            fi
            ;;
        "clean-cache")
            echo "--- Cleaning package cache ---"
            if command -v paccache > /dev/null 2>&1; then
                echo "Using paccache to keep last 2 versions..."
                pkexec paccache -r -k 2
            else
                echo "Using pacman -Sc (cleaning uninstalled packages)..."
                pkexec pacman -Sc --noconfirm
            fi
            # Also clean yay cache if it exists
            if [ -d "$HOME/.cache/yay" ]; then
                echo "Cleaning yay cache..."
                find "$HOME/.cache/yay" -type d -name "src" -exec rm -rf {} +
                find "$HOME/.cache/yay" -type d -name "pkg" -exec rm -rf {} +
            fi
            ;;
        *)
            echo "Invalid action: $ACTION"
            exit 1
            ;;
    esac
    echo ""
done

exit $EXIT_CODE
