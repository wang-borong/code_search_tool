#!/usr/bin/env bash

# Copyright (C) 2022 Jason Wang
# Github app installer

# like: neovim/neovim
function install_apps() {
    while [[ $1 != "" ]]; do
        github_repo=$1

        releases=$(curl -s https://api.github.com/repos/$github_repo/releases/latest \
            | grep browser_download_url | awk -F " " '{print $2}' | sed 's/"//g')
        if [[ -z $releases ]]; then
            echo "can't get $github_repo app"
            return
        fi
        select r in $releases; do
            appname=$(echo $r | awk -F 'download' '{print $2}' | awk -F '/' '{print $3}')
            echo "downloading $appname..."
            curl -O -L --output-dir /tmp $r
            if [[ $(echo $appname | grep ".tar.") != "" ]]; then
                if [[ $(tar tf /tmp/$appname | head -2 | grep '/') != "" ]]; then
                    mkdir -p ~/.opt
                    tar xvf /tmp/$appname -C ~/.opt
                elif [[ $(tar tf /tmp/$appname | head -1 | grep '/') != "" ]]; then
                    mkdir -p ~/.local/bin
                    mv /tmp/$(tar xvf /tmp/$appname) ~/.local/bin
                else
                    mkdir -p ~/.local/bin
                    tar xvf /tmp/$appname -C ~/.local/bin
                fi
            elif [[ $(echo $appname | grep ".deb") != "" ]]; then
                # left the error exist on non-debian distribution
                sudo dpkg -i /tmp/$appname
            else
                echo "Please install /tmp/$appname manually"
            fi
            break
        done
        shift
    done
}

function check_if_apps_exist() {
    for app in $@; do
        which $app >/dev/null
        if [[ $? != 0 ]]; then
            echo $app
        fi
    done
}

dist=$(lsb_release -a 2>&1 | grep "Distributor ID" | awk -F ' ' '{print $3}')
if [[ $(echo $dist | grep "Arch") != "" \
    || $(echo $dist | grep "Manjaro") != "" ]]; then
    apps=()
    [[ $(check_if_apps_exist "nvim") == "nvim" ]] &&
        apps+=" neovim "
    [[ $(check_if_apps_exist "rg") == "rg" ]] &&
        apps+=" ripgrep "
    apps+=$(check_if_apps_exist "bat" "fzf")

    yay -S $apps
else
    apps=()
    shrc=.$(echo $SHELL|awk -F'/bin/' '{print $2}')rc

    if [[ $(check_if_apps_exist "nvim") == "nvim" ]]; then
        install_apps "neovim/neovim"
        if [[ $(echo $PATH | grep "\.opt/nvim-linux64/bin") == "" ]]; then
            echo "PATH=\$PATH:~/.opt/nvim-linux64/bin" >> ~/$shrc
            echo "try NvChad configuration for neovim!"
        fi
    fi
    [[ $(check_if_apps_exist "rg") == "rg" ]] &&
        apps+=" BurntSushi/ripgrep "
    [[ $(check_if_apps_exist "bat") == "bat" ]] &&
        apps+=" sharkdp/bat "
    [[ $(check_if_apps_exist "fzf") == "fzf" ]] &&
        apps+=" junegunn/fzf "
    install_apps $apps

    if [[ $(echo $PATH | grep "\.local/bin") == "" ]]; then
        echo "PATH=\$PATH:~/.local/bin" >> ~/$shrc
    fi
fi

if [[ $(check_if_apps_exist "fs") == "fs" ]]; then
    install_apps wang-borong/code_search_tool
    mv ~/.local/bin/code-search ~/.local/bin/fs
fi

echo "you can use \"fs\" to hack your code now!"
echo "open a new terminal to use it!"
