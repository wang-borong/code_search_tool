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
                if [[ $(tar tf /tmp/$appname | head -1 | grep '/') != "" ]]; then
                    mkdir -p ~/.opt
                    sudo tar xvf /tmp/$appname -C ~/.opt
                else
                    mkdir -p ~/.local/bin
                    sudo tar xvf /tmp/$appname -C ~/.local/bin
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

echo "installing neovim ripgrep bat fzf"
dist=$(lsb_release -a | grep "Distributor ID" | awk -F ' ' '{print $3}')
if [[ $(echo $dist | grep "Arch") != "" \
    || $(echo $dist | grep "Manjaro") != "" ]]; then
    yay -S neovim ripgrep bat fzf
else
    install_apps neovim/neovim BurntSushi/ripgrep sharkdp/bat junegunn/fzf
    # add neovim path, bash and zsh
    shrc=.$(echo $SHELL|awk -F'/bin/' '{print $2}')rc
    if [[ $(echo $PATH | grep ".local/bin") == "" ]]; then
        echo "PATH=$PATH:~/.local/bin" >> ~/$shrc
    fi
    echo "PATH=$PATH:~/.opt/nvim-linux64/bin" >> ~/$shrc
    source ~/$shrc
    echo "try NvChad configuration for neovim!"
fi

echo "installing fs (code_search_tool)"
install_apps wang-borong/code_search_tool

echo "you can use \"fs\" to hack your code now!"
