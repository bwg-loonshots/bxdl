# 관리 CLI 독립 업데이트

이 문서의 `design/`·`results/`·다른 문서 상대 링크는 BXDL 저장소 기준이다. CLI-only 배포본의 문서는 `management-cli.md`와 `completion.md`이며, 설치·업데이트·자동완성 명령은 동봉 문서만으로 따라 할 수 있다.

관리 CLI는 노드 runtime과 별도로 설치·업데이트한다. Mac에서는 고정 RC.3·RC.4 package profile을, Linux 개발 경로에서는 등록 당시 root가 보호한 descriptor와 고정 worker를 사용한다. 관리 CLI를 교체해도 노드의 내부 실행기·NIGO JAR·JRE·DB를 교체하지 않는다. 플랫폼별 구현과 실제 인수 범위는 [현재 상태](./implementation-status.md)를 따른다.

```sh
/path/to/new/bxdl nodes add local --instance /path/to/control
/path/to/new/bxdl status local
/path/to/new/bxdl start local
/path/to/new/bxdl status local --wait
/path/to/new/bxdl logs local
/path/to/new/bxdl diagnose local --output /path/to/new-report.json
/path/to/new/bxdl stop local
```

별도 목록을 쓰면 이름 명령에도 동일한 `--home`을 지정한다. 이름과 `--instance`는 둘 중 하나만 받는다. 기존 경로 명령은 catalog가 없어도 동작하며 `--home`을 섞지 않는다. 관리 CLI를 바꿔도 사용자 목록·control·data를 다시 만들지 않는다.

개발 checkout에서는 `make build`로 만든 `bin/bxdl`을 외부 관리 CLI로 사용한다. 배포 시에도 package 밖의 관리 CLI 실행 파일만 교체한다. `bxdl version --json`의 version/revision은 현재 관리 CLI의 식별 정보이며 NIGO 엔진 버전과 별도로 관리한다. 기존 이름 목록은 같은 사용자/관리 home에서 이어 읽는다.


## 플랫폼과 Linux 권한 경계

독립 관리 CLI의 배포·설치·업데이트·복귀는 Mac arm64와 Linux x86_64/glibc에 구현되어 있다.
Ubuntu CI의 native CLI 빌드·자동 검사는 통과했으며 실제 Linux 설치·초기화·systemd 운용·CLI 전환 인수는 별도다. Linux 개발용 노드 설치·운용 절차는 [Linux 안내](./linux-operations.md)를 따른다.
`version --json`의 `primaryTarget`은 실행 바이너리 대상(`darwin-arm64` 또는 `linux-amd64`)이며,
Linux는 `management.install.linux`·`management.update.linux`·`management.rollback.linux`를 표시한다.
Mac의 고정 RC.3/RC.4 실행기 capability를 Linux 지원 근거로 표시하지 않는다.

Linux의 기본 CLI root는 `~/.local/share/BXDL-CLI`, 실행 진입점은 `~/.local/bin/bxdl`이다.
`XDG_DATA_HOME`을 자동 채택하지 않으며 다른 위치는 `--root`·`--bin-dir`로 명시한다.
노드 이름·설치 준비를 저장하는 사용자 목록의 기본 위치는 `~/.local/share/BXDL`이며 `--home`으로 바꾼다. 관리 CLI root와 사용자 목록, node control·DB는 별개의 저장소다. Linux의 PATH 안내는 기본 셸에서 사용할 수 있는 다음 형태다.

```sh
export PATH="$HOME/.local/bin:$PATH"
hash -r
command -v bxdl
bxdl version --json
bxdl cli info
bxdl host check --json
```

CLI-only 설치와 `cli update/rollback` 문법·서명 신뢰·기존 선택 보존은 두 OS에서 공통이다.
해당 OS의 배포본만 설치하며 다른 OS/CPU의 배포본은 실행·새 root 생성 전에 거절한다.
Linux 배포본에는 ELF64/x86_64와 glibc interpreter가 필요하다. 같은 version/revision이라도 다른 target 배포본을 섞지 않는다.
자동완성 배포물은 계속 zsh용이며 Bash 자동완성은 아직 제공하지 않는다. 셸 설정 파일은 자동으로 바꾸지 않는다.

Linux 빌더에서 `tools/build-management-cli.py --target linux-amd64`를 사용한다.
기본 target은 실행 중인 지원 호스트이며 빌더의 OS/CPU와 다른 target은 빌드 전에 거절한다.
실제 clean source의 버전·revision·target을 확인한 배포본을 만들어야 하며, 개발 모드와 서명 정책은 기존과 같다.
Mac 빌더의 Linux 타입 검사를 Linux 바이너리 링크·실행·인수로 기록하지 않는다.
Linux 운용은 일반 사용자 CLI가 root 소유 `management.json`의 control identity·protocol·worker hash를 검증하고 최초 설치 때 고정한 worker로 정해진 요청만 위임한다. `setup apply`는 TTY 승인 뒤 관리자 인증을 요청하며, init/start/status/stop/logs/diagnose는 기존 sudo 인증을 사용한다. 인증이 없거나 만료됐으면 `sudo -v` 후 다시 실행한다. 명령 전체를 `sudo bxdl ...`로 실행해 사용자 catalog를 root home과 섞지 않는다.

Linux의 `linux-executor.development.v1`은 개발용 descriptor 프로토콜이다. Mac의 exact archive profile이나 정식 Linux runtime 인수와 구분한다. 관리 CLI 업데이트는 기존 worker를 바꾸거나 구형 Linux control에 descriptor를 자동 추가하지 않는다.

## 관리 CLI 설치

해당 OS용 CLI 전용 배포본으로 사용자 권한의 `cli install/update/info/rollback`을 실행한다. 관리 CLI 배포본에는 JRE나 NIGO 엔진이 없으므로 Rust/Cargo, Java 또는 노드 실행 없이 CLI를 설치할 수 있다. 기존 노드용 `bxdl install`은 유지한다.

공급받은 배포본을 새 디렉터리에 푼 뒤 그 안의 실행 파일로 최초 설치한다. 서명 배포본의 공개키는 배포본과 별도로 신뢰한 경로에서 받는다. 다음 `./bxdl-cli`는 풀어 둔 디렉터리의 예시다.

```zsh
./bxdl-cli/bin/bxdl cli install --public-key /trusted/bxdl-cli-public.pem
```

`--from`을 생략하면 지금 실행한 바이너리를 포함한 배포 디렉터리를 찾는다. checkout에서 빌드한 단독 바이너리에는 이 방식이 적용되지 않는다. 이미 신뢰하는 새 관리 CLI가 있으면 archive 또는 풀어 둔 디렉터리를 직접 지정할 수도 있다.

```zsh
/path/to/bxdl cli install --from /downloads/bxdl-cli.tar.gz --public-key /trusted/bxdl-cli-public.pem
```

기본 경로는 다음과 같다. `sudo`가 필요하지 않으며 셸 설정 파일을 자동으로 편집하지 않는다.

| 용도 | Mac | Linux |
| --- | --- | --- |
| CLI 배포본·선택 기록 | `~/Library/Application Support/BXDL-CLI` | `~/.local/share/BXDL-CLI` |
| PATH 실행 진입점 | `~/.local/bin/bxdl` | `~/.local/bin/bxdl` |
| 자동완성 | `<CLI root>/current/share/zsh/site-functions` | `<CLI root>/current/share/zsh/site-functions` |
| 노드 목록·준비 기록 | `~/Library/Application Support/BXDL` | `~/.local/share/BXDL` |

CLI 위치는 `--root`/`--bin-dir`, 노드 목록 위치는 `--home`으로 지정한다. 서로 다른 저장소다. 사용자 자료가 있는 폴더나 노드·runtime package를 CLI root로 선택하지 않는다. 기존 `bin-dir/bxdl`이 이 설치가 관리하는 링크가 아니면 덮어쓰지 않는다.

zsh 터미널에서는 설치한 CLI와 자동완성을 다음처럼 사용한다. Bash에서는 `rehash` 대신 `hash -r`을 사용하고 compinit/source 단계는 생략한다.

```zsh
export PATH="$HOME/.local/bin:$PATH"
rehash
command -v bxdl
bxdl version --json
bxdl cli info
autoload -Uz compinit
compinit
source <(bxdl completion zsh)
```

`command -v bxdl`의 경로가 원하는 진입점인지 확인한다. `cli info`는 CLI root·실행 진입점·이번 명령을 실행한 파일·현재와 이전 버전·PATH의 파일 우선순위를 표시한다. 셸의 alias나 함수까지 검사하지는 않으므로 다른 결과가 나오면 `type -a bxdl`로 셸 설정을 확인한다. 아직 설치하지 않았으면 `NOT_INSTALLED`를 성공 응답으로 돌려주며 폴더를 만들지 않는다.

Mac 기본 CLI root를 새 zsh 터미널에도 적용하려면 자신의 `.zshrc`에서 다음 PATH/fpath를 설정한다. fpath는 기존 `compinit` 호출 앞에 넣는다. 이미 초기화하는 설정이 있다면 `compinit`을 중복 추가하지 않는다.

```zsh
export PATH="$HOME/.local/bin:$PATH"
fpath=("$HOME/Library/Application Support/BXDL-CLI/current/share/zsh/site-functions" $fpath)
autoload -Uz compinit
compinit
```

Linux 기본 CLI root의 fpath는 `$HOME/.local/share/BXDL-CLI/current/share/zsh/site-functions`다. 다른 root를 사용했다면 `cli info`에 표시된 경로를 사용한다. 이 고정 경로는 선택한 CLI와 함께 전환된다. 다른 디렉터리에 복사한 예전 `_bxdl`이 먼저 로드되지 않도록 fpath 순서를 확인한다. 현재 셸이 이미 읽은 완성 함수는 업데이트 뒤 `source <(bxdl completion zsh)`로 다시 읽는다. [자동완성 가이드](./completion.md)에서 Tab 예시를 볼 수 있다.

## 업데이트와 복귀

첫 서명 설치 때 공개키를 CLI root에 고정한다. 이후 업데이트는 그 키로 검증하므로 공개키 옵션을 다시 입력하지 않아도 된다. 인터넷에서 최신 버전을 찾거나 다운로드하지 않으며 공급받은 로컬 archive/디렉터리를 명시한다.

```zsh
bxdl cli update --from /downloads/bxdl-cli-next.tar.gz
bxdl version --json
bxdl cli info
source <(bxdl completion zsh)
```

새 배포본의 manifest·inventory·파일 hash·서명을 확인하고 바이너리의 제한된 `version --json`을 대조한 뒤 현재 선택을 전환한다. CLI와 자동완성은 같은 버전으로 바뀐다. 같은 version/revision의 같은 배포본 재요청은 변경하지 않으며, 같은 identity에 다른 파일을 넣은 공급물은 거부한다. 키나 신뢰 정책을 자동으로 바꾸지 않는다.

이전 선택은 보존한다. 문제가 있으면 다음 명령으로 돌아간다.

```zsh
bxdl cli rollback
bxdl version --json
source <(bxdl completion zsh)
```

복귀도 저장한 이전 배포본의 무결성을 검사한다. CLI 복귀는 노드 엔진의 rollback이나 DB 복구를 수행하지 않는다. 이전 버전이 없거나 저장 파일이 손상됐다면 임의 버전을 선택하지 않고 오류를 반환한다.

별도 위치를 쓰면 최초 설치에 두 위치를 함께 정한다. 이후에는 같은 `--root`만 지정하면 저장한 실행 진입점 위치를 이어 쓴다. 기존 설치에 다른 `--bin-dir`을 지정하는 이동은 지원하지 않는다.

```zsh
/path/to/bxdl cli install --from /downloads/bxdl-cli.tar.gz \
  --root "$HOME/team-cli" --bin-dir "$HOME/team-bin" \
  --public-key /trusted/bxdl-cli-public.pem
"$HOME/team-bin/bxdl" cli info --root "$HOME/team-cli"
"$HOME/team-bin/bxdl" cli update --root "$HOME/team-cli" --from /downloads/bxdl-cli-next.tar.gz
"$HOME/team-bin/bxdl" cli rollback --root "$HOME/team-cli"
```

실행 명령이 작업 요청이며 번호 메뉴나 TTY 문답을 추가하지 않는다. 자동화에서는 각 명령에 `--json`을 붙인다. 검증·권한·경로 충돌로 전환 전에 실패하면 기존 선택을 유지한다. 오류 뒤에는 `cli info`와 `version --json`으로 상태를 확인한다. 선택 결과를 확정할 수 없다는 오류는 성공으로 취급하지 않는다. 저장소 파일이나 링크를 수동으로 바꿔 오류를 숨기지 않는다.

## 개발 배포본과 제작

서명 없는 개발 배포본은 설치와 업데이트 **매번** 명시 opt-in이 필요하다. signed root의 검증을 우회하는 옵션이 아니다.

```zsh
./bxdl-cli/bin/bxdl cli install --allow-unsigned-development
bxdl cli update --from /downloads/bxdl-cli-dev-next.tar.gz --allow-unsigned-development
```

배포 제작자는 clean commit에서 정확한 Git revision을 포함하는 CLI-only archive를 만든다. 출력 파일은 새 경로여야 하며 기존 파일을 교체하지 않는다. private key는 archive에 포함하지 않는다.

```sh
python3 tools/build-management-cli.py --output /output/bxdl-cli.tar.gz --signing-key /private/cli-signing.pem
```

수정 중인 checkout에서는 개발 모드로 빌드할 수 있다. 이때 revision은 `development`이고 서명할 수 없다. 같은 development identity로 내용이 달라진 배포본을 기존 선택 위에 설치할 수 없으므로 서로 다른 개발 실험은 별도 임시 CLI root에서 확인한다.

```sh
python3 tools/build-management-cli.py --development --allow-unsigned-development --output /output/bxdl-cli-dev.tar.gz
```

저수준 제작·검증 명령은 `bxdl cli bundle build --help`와 `bxdl cli bundle verify --help`를 따른다. 검증만 하려면 다음처럼 실행한다. `verify`는 배포본을 설치하거나 후보 실행 파일을 실행하지 않는다.

```sh
bxdl cli bundle verify /downloads/bxdl-cli.tar.gz --public-key /trusted/bxdl-cli-public.pem --json
```

이 형식의 Ed25519 manifest 서명과 SHA-256 검사는 Apple code signing·notarization이나 정식 고객 배포 인수를 대신하지 않는다. `.pkg`/Homebrew, 자동 온라인 업데이트와 설치 제거는 제공하지 않는다. Linux 비공개 시험은 [고정 소스 빌드와 준비 도구](./linux-native-build.md)를 사용하며, 명령 구현·자동 검사·실제 노드 인수는 [지원 범위](./support-matrix.md)에서 구분한다.

## 지원 범위

Mac은 고정 RC.3·RC.4 archive·manifest·실행기·JAR·Java 조합을 각각 인증한다. 같은 버전 문자열이나 임의 재빌드는 지원 profile에 자동 포함되지 않는다. 전체 identity는 [실행기 분리 설계](../design/2026-10-23-independent-management-cli.md), [RC.4 runtime 조립 기록](../results/2026-11-15-rc4-runtime-assembly.md)에 고정되어 있다.

지원하는 Mac 등록의 `instance show`, `preflight`, `init`/`resume-init`, `start`, `status`/`--wait`, `stop`, `logs`, `diagnose`를 고정 실행기에 위임한다. init/resume-init의 명시 확인 플래그는 유지한다. 최초 설치·등록과 `setup apply`는 별도 승인 범위이며 초기화·시작을 설치 승인에 묶지 않는다. 미지원 package에는 기존 same-CLI 검증 조건을 적용하고 임의 실행기를 대신 실행하지 않는다.

Linux는 [Linux 설치·운용](./linux-operations.md)의 등록 descriptor와 최초 worker를 대조한다. 독립 관리 CLI 설치 성공만으로 실제 systemd 서비스 인수나 버전 전환 호환이 검증된 것은 아니다. 현재 비공개 시험의 최초 management/worker bytes는 같으며 별도 버전 전환은 후속이다.

## 동작과 결과

service의 READY는 로컬 상태이며 전역 합의의 확인이 아니다. logs/diagnose는 저장 기록을 수집하며 현재 runtime을 관측하지 않는다. 누락·권한 오류·손상·수집 한도는 partial로 표시한다. 특히 기존 `0644` 초기화 보고서는 자동 복구하지 않아 새 CLI에서도 partial일 수 있다. 파일이 생성됐다는 사실만으로 완전 수집으로 판단하지 않는다. Mac 위임 전 archive와 설치 inventory 인증 비용이 있으므로 `--timeout-seconds`를 전체 명령의 벽시계 상한으로 해석하지 않는다.

작업이 실행기에 전달된 뒤 결과를 확정할 수 없으면 변경 가능 작업은 UNKNOWN으로 보존하고 자동 재시도·초기화·start하지 않는다. Mac 위임의 대표 코드는 `EXECUTOR_OPERATION_UNKNOWN`/exit 6이며 Linux 사유 코드는 [Linux 안내](./linux-operations.md)를 따른다. 관리 CLI 종료/timeout은 노드 중지나 작업 취소를 보장하지 않는다. `status`와 기존 기록을 확인한다.

초기화·시작 journal과 worker snapshot은 각 고정 실행기가 관리한다. 구형 Mac 실행기의 stop은 잠금을 얻었을 때의 실행 시도를 대상으로 한다. 관리 CLI는 이름의 root/binding을 직전 재검사하지만 구형 실행기가 지원하지 않는 원자적 expected-attempt 승인 전달을 추가한 것으로 간주하지 않는다.

## 파일 생성 권한과 초기화 보고서

standalone `bxdl`은 Unix 프로세스 진입 시 `umask 077`을 설정하고 자식 실행기와 JVM에 상속한다. 호출 셸의 mask나 기존 파일 권한은 바꾸지 않는다. 새 init/resume-init 보고서는 private `0600`으로 생성한다. 라이브러리 API를 직접 호출하거나 이 정책이 없는 구형 내장 CLI를 직접 실행하는 경우에는 같은 정책이 자동 적용되지 않는다.

기존 `0644` 보고서를 chmod·덮어쓰기하거나 수집 목적으로 초기화를 재실행하지 않는다. private reader의 권한 검사는 유지하고 누락 사유와 partial 상태를 보존한다. [권한 설계](../design/2026-10-23-private-initialization-report.md)와 [Mac 검증 근거](../results/2026-10-23-private-initialization-report.md)를 참고한다.

새 `package build` archive의 외부 파일은 `0600`이며 내부 payload bytes/mode와 설치 시 실행 파일 `0755`·일반 payload `0644`는 유지한다. 셸의 `>` 리다이렉션 파일은 호출 셸이 먼저 열므로 이 정책의 대상이 아니다.

## 업데이트 기준

- 이름·도움말·표시·문답 개선은 관리 CLI를 교체하고 지원 runtime과의 호환 조합을 검사한다.
- 실행기 동작·보안 수정 또는 NIGO/JRE 변경은 새 runtime package를 만들고 인수한다.
- 기존 노드 DB·control 이동·upgrade/migration은 별도 작업이다. 관리 CLI 교체가 이를 수행하지 않는다.

Mac 사용자 LaunchAgent와 Linux 개발용 systemd의 범위는 각각 유지된다. CLI 업데이트가 별도 상주 관리 서버·자동 재시작·부팅 자동 시작을 추가하지 않는다.

## RC.4 고정 런타임 지원과 CLI 전환 기준

Mac 관리 CLI의 설치·업데이트·복귀 probe는 `executor.rc3-legacy-v1`과 `executor.rc4-v1` capability를 모두 요구한다. 하나라도 없으면 `CLI_CANDIDATE_FAILED`로 거절하고 기존 CLI 선택을 유지한다. RC.3 전용 구형 CLI를 RC.4 운용 중 정상 복귀 대상으로 사용하지 않는다. 이미 배포된 구형 bootstrap의 검사 정책을 소급 변경하지는 않는다.

Linux 관리 CLI probe는 Linux 관리 기능과 zsh completion capability를 확인하며 Mac exact runtime capability를 요구하지 않는다. 이 차이는 Linux runtime profile을 자동 승인한다는 뜻이 아니다.

Mac 지정 RC.4 조합의 단일노드·동종망·실행 중 CLI 전환 근거는 [서비스 결과](../results/2026-11-15-rc4-macos-service.md)와 [네트워크 결과](../results/2026-11-16-rc4-macos-network.md)를 따른다. 다른 기능 변경 CLI나 Linux 조합에 그 결과를 승계하지 않는다. `cli update/rollback`은 노드 엔진·JRE·DB의 업그레이드·복구 명령이 아니다.

### Linux에서 기존 진입점 디렉터리의 권한 확인

일반 사용자로 `bxdl cli install`을 실행할 때 기존 `~/.local/bin`이나 상위 디렉터리에 그룹 쓰기 권한이 있으면 설치가 중단될 수 있다. 터미널의 경로와 권한값을 확인한다. 본인이 소유하고 개인용으로 사용하는 폴더만 표시된 명령으로 조정한다. 예를 들어 `~/.local/bin`의 `0775`는 `chmod -- go-w "$HOME/.local/bin"`으로 `0755`가 된다. 공유 디렉터리라면 최초 설치에서 안전한 개인 경로를 `--bin-dir`로 지정하고 그 경로를 PATH에 추가한다. 배포 저장소 경로를 따로 정할 때는 `--root`를 사용한다.
