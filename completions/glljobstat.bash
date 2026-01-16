# Bash completion script for glljobstat
# Install: copy to /etc/bash_completion.d/glljobstat or source in ~/.bashrc

_glljobstat_get_profiles() {
    local configfile="${1:-$HOME/.glljobstat.toml}"
    if [[ -f "$configfile" ]]; then
        grep -oP '^\[profile\.\K[^\]]+' "$configfile" 2>/dev/null
    fi
}

_glljobstat_completions() {
    local cur prev opts
    COMPREPLY=()
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"

    # All options
    opts="-C --configfile
          -P --profile
          -c --count
          -i --interval
          -n --repeats
          --param
          --groupby
          --sortby
          -o --ost
          -m --mdt
          -s --servers
          --fullname
          -f --filter
          -F --fmod
          -l --length
          -t --total
          -T --totalrate
          --minrate
          --totalratefile
          -p --percent
          -H --humantime
          --num-proc-ssh
          --num-proc-data
          --hist
          -v --verbose
          -d --difference
          -r --rate
          --log-raw-data
          --log-data-victoriametrics
          --log-data-prometheus
          --log-data-parquet
          --log-only
          --log-max-size
          --tui
          --list-profiles
          -h --help
          -V --version"

    # Handle option arguments
    case "${prev}" in
        -C|--configfile)
            # Complete with .toml files
            COMPREPLY=( $(compgen -f -X '!*.toml' -- "${cur}") $(compgen -d -- "${cur}") )
            return 0
            ;;
        -P|--profile)
            # Find config file from command line or use default
            local configfile="$HOME/.glljobstat.toml"
            for ((i=1; i < COMP_CWORD; i++)); do
                if [[ "${COMP_WORDS[i]}" == "-C" || "${COMP_WORDS[i]}" == "--configfile" ]]; then
                    configfile="${COMP_WORDS[i+1]}"
                    break
                fi
            done
            local profiles=$(_glljobstat_get_profiles "$configfile")
            COMPREPLY=( $(compgen -W "${profiles}" -- "${cur}") )
            return 0
            ;;
        --groupby)
            COMPREPLY=( $(compgen -W "none user group host host_short job proc" -- "${cur}") )
            return 0
            ;;
        --sortby)
            COMPREPLY=( $(compgen -W "ops open close mknod link unlink mkdir rmdir rename getattr setattr getxattr setxattr statfs sync samedir_rename crossdir_rename read_bytes write_bytes punch" -- "${cur}") )
            return 0
            ;;
        --param)
            COMPREPLY=( $(compgen -W "*.*.job_stats obdfilter.*.job_stats mdt.*.job_stats" -- "${cur}") )
            return 0
            ;;
        --tui)
            # Complete with supported replay file types
            COMPREPLY=( $(compgen -f -X '!*.@(raw.log|parquet|prom|vm.json)' -- "${cur}") $(compgen -d -- "${cur}") )
            return 0
            ;;
        --log-raw-data|--log-data-victoriametrics|--log-data-prometheus|--log-data-parquet)
            # Complete with directories and files
            COMPREPLY=( $(compgen -f -- "${cur}") $(compgen -d -- "${cur}") )
            return 0
            ;;
        --totalratefile)
            # Complete with .json files
            COMPREPLY=( $(compgen -f -X '!*.json' -- "${cur}") $(compgen -d -- "${cur}") )
            return 0
            ;;
        --log-max-size)
            # Suggest common sizes
            COMPREPLY=( $(compgen -W "10M 50M 100M 500M 1G 5G 10G" -- "${cur}") )
            return 0
            ;;
        -c|--count|-i|--interval|-n|--repeats|-l|--length|--minrate|--num-proc-ssh|--num-proc-data)
            # These take numbers, no completion
            return 0
            ;;
        -s|--servers|-f|--filter)
            # These take user-specified values, no completion
            return 0
            ;;
    esac

    # Complete options
    if [[ "${cur}" == -* ]]; then
        COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
        return 0
    fi
}

complete -F _glljobstat_completions glljobstat

