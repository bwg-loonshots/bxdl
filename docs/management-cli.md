# 관리 CLI 독립 업데이트

이 문서의 `design/`·`results/`·다른 문서 상대 링크는 BXDL 저장소 기준이다. CLI-only 배포본의 문서는 `management-cli.md`와 `completion.md`이며, 설치·업데이트·자동완성 명령은 동봉 문서만으로 따라 할 수 있다.

새 관리 CLI를 설치 package 밖에서 실행해도 검증된 기존 RC.3 package를 운용할 수 있다. package 내부의 `bin/bxdl`·NIGO JAR·JRE를 덮어쓰거나 archive를 다시 만들지 않는다. 관리 CLI는 대상과 입력을 검사한 뒤 해당 package의 실행기에 고정 명령·JSON으로 위임한다.

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

## Mac 관리 CLI 설치

CLI05-02는 Mac arm64용 CLI 전용 배포본과 사용자 권한의 `cli install/update/info/rollback`을 제공한다. 검증 범위와 현재 상태는 [구현 현황](./implementation-status.md)을 따른다. 관리 CLI 배포본에는 JRE나 NIGO 엔진이 없으므로 Rust/Cargo, Java 또는 노드 실행 없이 CLI를 설치할 수 있다. 기존 노드용 `bxdl install`은 유지한다.

공급받은 배포본을 새 디렉터리에 푼 뒤 그 안의 실행 파일로 최초 설치한다. 서명 배포본의 공개키는 배포본과 별도로 신뢰한 경로에서 받는다. 다음 `./bxdl-cli`는 풀어 둔 디렉터리의 예시다.

```zsh
./bxdl-cli/bin/bxdl cli install --public-key /trusted/bxdl-cli-public.pem
```

`--from`을 생략하면 지금 실행한 바이너리를 포함한 배포 디렉터리를 찾는다. checkout에서 빌드한 단독 바이너리에는 이 방식이 적용되지 않는다. 이미 신뢰하는 새 관리 CLI가 있으면 archive 또는 풀어 둔 디렉터리를 직접 지정할 수도 있다.

```zsh
/path/to/bxdl cli install --from /downloads/bxdl-cli.tar.gz --public-key /trusted/bxdl-cli-public.pem
```

기본 경로는 다음과 같다. `sudo`가 필요하지 않으며 셸 설정 파일을 자동으로 편집하지 않는다.

| 용도 | 기본 경로 |
| --- | --- |
| CLI 배포본·선택 기록 | `~/Library/Application Support/BXDL-CLI` |
| PATH에 넣을 실행 진입점 | `~/.local/bin/bxdl` |
| 선택한 CLI의 자동완성 | `~/Library/Application Support/BXDL-CLI/current/share/zsh/site-functions` |
| 기존 노드 목록·준비·운용 기록 | `~/Library/Application Support/BXDL` |

CLI 위치는 `--root`/`--bin-dir`, 노드 목록 위치는 `--home`으로 지정한다. 서로 다른 저장소다. 사용자 자료가 있는 폴더나 노드·runtime package를 CLI root로 선택하지 않는다. 기존 `bin-dir/bxdl`이 이 설치가 관리하는 링크가 아니면 덮어쓰지 않는다.

현재 터미널에서 설치한 CLI와 자동완성을 사용한다.

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

새 터미널에도 적용하려면 자신의 `.zshrc`에서 다음 PATH/fpath를 설정한다. fpath는 기존 `compinit` 호출 앞에 넣는다. 이미 초기화하는 설정이 있다면 `compinit`을 중복 추가하지 않는다.

```zsh
export PATH="$HOME/.local/bin:$PATH"
fpath=("$HOME/Library/Application Support/BXDL-CLI/current/share/zsh/site-functions" $fpath)
autoload -Uz compinit
compinit
```

이 고정 경로는 선택한 CLI와 함께 전환된다. 다른 디렉터리에 복사한 예전 `_bxdl`이 먼저 로드되지 않도록 fpath 순서를 확인한다. 현재 셸이 이미 읽은 완성 함수는 업데이트 뒤 `source <(bxdl completion zsh)`로 다시 읽는다. [자동완성 가이드](./completion.md)에서 Tab 예시를 볼 수 있다.

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

이 형식의 Ed25519 manifest 서명과 SHA-256 검사는 Apple code signing·notarization이나 정식 고객 배포 인수를 대신하지 않는다. 이번 단계의 배포 대상은 Mac arm64이며 `.pkg`/Homebrew, 자동 온라인 업데이트, 설치 제거, Linux 실제 설치는 후속 범위다. 후속 CLI05-03의 지정 서비스·네트워크 조합 인수는 [별도 결과](../results/2026-10-23-cli05-runtime-acceptance.md)를 따른다. 후속 [초기화 보고서 권한 개선과 재검증](../results/2026-10-23-private-initialization-report.md)은 신규 보고서를 대상으로 완료했다. 기존 `0644` 보고서 복구와 작성자가 아닌 사용자의 CLI05-04 과업은 완료하지 않았으며 사람 과업은 NOT_RUN이다.

## 지원 범위

첫 호환 profile은 [RC.3 인수 기록](../results/2026-10-22-macos-rc3-package.md)의 archive `8a45cb8b…`, 실행기 `b7d677b4…` 조합이다. 다른 RC.3 재빌드나 같은 `0.1.0-dev` 문자열은 이 profile에 자동 포함되지 않는다. [분리 설계](../design/2026-10-23-independent-management-cli.md)에 전체 hash·계약을 기록했다.

등록된 RC.3의 `instance show`, `preflight`, `init`/`resume-init`, `start`, `status`/`--wait`, `stop`, `logs`, `diagnose`를 위임한다. init/resume-init은 기존 명시 확인 플래그가 필요하다. 최초 설치와 등록은 기존 API를 사용하며, `setup --install`도 이 RC.3를 선택해 노드 작업을 내부 실행기에 위임할 수 있다. `setup new/resume`은 순차 문답으로 자료를 준비하며, `setup apply <이름>`은 TTY 승인 뒤 기존 installer/등록 API로 설치·cold 검사·등록·이름 연결까지 진행한다. `init <이름> --confirm-initialize`와 조건부 `resume-init <이름> --confirm-resume`도 이름 또는 기존 `--instance`를 선택해 같은 고정 RC.3 실행기에 위임한다. 초기화와 start를 설치 승인에 묶지 않는다.

미지원 package는 새 CLI가 임의로 내부 실행기를 실행하지 않는다. 기존 native 경로의 조건을 적용하며, 특히 start는 기존과 같이 package CLI 일치를 요구한다. 지원 profile에서 서명·파일 검증이 실패하면 실행 방식 변경이나 자동 복구 없이 거절한다.

[기존 Mac 검증](../results/2026-10-23-independent-management-cli.md)에서는 두 관리 CLI로 같은 RC.3의 직접 운용16사례와 설치 도우미 재개8사례를 확인했다. 당시 READY는 단일 노드의 MANUAL_START_INITIALIZED이며 합의·거래 확정 근거가 아니었다.

후속 [CLI05-03 조합 인수](../results/2026-10-23-cli05-runtime-acceptance.md)는 실제 설치한 진입점으로 서비스 16사례와 4-validator 네트워크를 수행했다. 같은 source tree의 PR head `994c48f`와 merge `3a0bb5c` 배포본을 실행 중 3회 전환하면서 동일 attempt/PID·data·초기화 journal·설정·고정 worker를 확인했고, RPC/monitor 근거로 지정 거래·두 복구·wrong-pin을 별도 판정했다. 이 검증도 `status`의 로컬 READY를 전역 합의 판정으로 바꾸지 않는다.

## 동작과 결과

명령·JSON envelope·exit 의미는 기존 계약을 유지한다. service의 READY는 로컬 상태이며 합의 전체의 확인이 아니다. logs/diagnose는 저장 기록이고 현재 runtime을 관측하지 않는다. CLI05-03 단일 노드 실행에서는 init report mode 0644와 private reader의 읽기 거절을 관측했고, `logs`·`diagnose`가 `LOGS_PARTIAL`·`DIAGNOSTIC_PARTIAL` / INCOMPLETE를 반환했다. 해당 실행의 실제 umask는 기록하지 않았으므로 당시 원인은 미확정으로 보존한다. 후속 조사는 보고서 writer의 생성 권한이 상속된 mask에 의존하는 경로를 확인했고, 새 관리 CLI 아래의 신규 보고서 생성·수집을 보완했다. 기존 `0644` 보고서는 그대로 보존하므로 새 CLI로 조회해도 partial일 수 있다. partial 보고서가 생성돼도 모든 기록을 읽은 것으로 해석하지 않는다. 위임 전 archive와 설치 inventory 인증 비용도 있으므로 `--timeout-seconds`를 전체 명령의 벽시계 상한으로 해석하지 않는다.

작업이 실행기에 전달된 뒤 결과를 확정할 수 없으면 변경 가능 작업은 `EXECUTOR_OPERATION_UNKNOWN`/exit 6으로 종료한다. 원문 출력은 노출하지 않으며 자동으로 재시도·초기화·start하지 않는다. 관리 CLI 종료/timeout은 노드 중지나 작업 취소를 보장하지 않는다. `status`와 기존 기록을 확인한다.

노드의 실제 초기화·시작 journal과 worker snapshot은 기존 실행기가 작성한다. 구형 실행기의 stop은 잠금을 얻었을 때 현재 실행 시도를 대상으로 한다. 관리 CLI는 이름의 root/binding을 직전 재검사하지만, 구형 실행기가 지원하지 않는 원자적 expected-attempt 승인 전달을 제공한다고 주장하지 않는다.

## 파일 생성 권한과 초기화 보고서

새 standalone `bxdl`은 Unix 프로세스의 첫 진입에서 `umask 077`을 설정한다. 호출한 셸의 mask는 바꾸지 않으며, 이후 직접 실행하는 JVM과 지원 RC.3의 고정 실행기/JVM에는 `077`이 상속된다. 새 init/resume-init 시도의 보고서를 처음부터 `0600`으로 만들기 위한 정책이다. 초기화 승인·결과 판정·UNKNOWN 처리와 명령/JSON 계약은 유지한다. [설계](../design/2026-10-23-private-initialization-report.md)와 [검증 결과](../results/2026-10-23-private-initialization-report.md)를 따른다.

라이브러리 API를 직접 호출할 때는 process-wide mask를 바꾸지 않는다. 기존 패키지의 내장 CLI를 관리 CLI 없이 직접 실행하는 경우에도 새 정책이 추가되지 않는다. 관리 CLI를 업데이트해도 기존 `0644` 보고서를 chmod·덮어쓰기하거나 초기화를 다시 실행하지 않는다. private reader의 `0600` 검사는 유지되므로 기존 보고서의 누락 사유와 partial 상태를 보존한다. 수집만을 위해 초기화를 재실행하지 않는다.

이 정책은 새 CLI가 생성하는 파일 전체에 적용된다. 새 노드 `package build` archive의 외부 파일은 `0600`으로 생성되며 archive 내부 payload의 bytes·mode와 설치 시 명시하는 실행 파일 `0755`·일반 payload `0644`는 유지한다. 기존 archive의 권한은 바꾸지 않는다. 셸의 `>` 리다이렉션으로 만든 출력 파일은 호출한 셸이 먼저 열기 때문에 이 정책의 대상이 아니다.

실제 Mac 비교 검증은 4개 실행 조건의 서비스16사례와 독립 검사를 구분해 기록했다. 수정 CLI 아래에서 새로 생성한 초기화 보고서의 private 권한과 수집 성공을 확인했지만, 기존 파일 복구나 모든 누락·손상 원인의 해소를 뜻하지 않는다. CLI05-04 사용자 과업은 계속 NOT_RUN이다.

## 업데이트 기준

- 이름·도움말·표시·문답 개선: 관리 CLI만 교체하고 지원 runtime과의 호환 조합을 검사한다.
- 실행기 동작/보안 수정 또는 NIGO/JRE 변경: 새 runtime package를 만들고 인수한다.
- 기존 노드 DB·control 이동·upgrade/migration은 별도 작업이다. 관리 CLI 교체가 이를 수행하지 않는다.

현재의 사용자 LaunchAgent 운영 범위는 그대로다. 별도 상주 관리 서버나 자동 재시작 기능을 추가하지 않는다.
