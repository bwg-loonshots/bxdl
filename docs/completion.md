# zsh 자동완성

CLI05-01에서 zsh 자동완성을 구현·검증했다. 전체 검사와 최종 release의 실제 zsh 결과는 [검증 기록](../results/2026-10-23-cli05-zsh-completion.md)을 따른다. 현재는 zsh를 지원하며 bash/fish는 지원하지 않는다.

`bxdl completion zsh`는 자동완성 스크립트를 표준출력으로 내보낸다. 셸 설정이나 파일을 자동으로 바꾸지 않으며 비TTY에서도 사용할 수 있다. 이 명령에는 `--json`을 붙이지 않는다. 함께 요청하면 사용법 오류와 JSON 오류 envelope를 반환한다.

## 현재 터미널에서 사용하기

먼저 새 관리 CLI를 실행할 수 있도록 준비한다. 개발 checkout의 저장소 루트에서는 다음과 같이 빌드하고 이 터미널의 PATH에 추가할 수 있다.

```zsh
make build
export PATH="$PWD/bin:$PATH"
bxdl version
```

zsh의 자동완성을 초기화한 뒤 스크립트를 읽는다. 이미 `compinit`을 실행하는 셸 설정을 사용한다면 마지막 줄만 실행하면 된다.

```zsh
autoload -Uz compinit
compinit
source <(bxdl completion zsh)
```

이 적용은 현재 셸에만 유지된다. 기존 RC.3 패키지 안의 `bin/bxdl`은 자동완성 기능을 포함하지 않으므로 [독립 관리 CLI](./management-cli.md)를 PATH에서 선택한다. 스크립트는 Tab을 누른 명령줄의 BXDL 실행 파일에서 후보를 조회하므로 CLI를 교체할 때 스크립트도 다시 읽는다.

다음 예시의 `[Tab]`은 글자로 입력하지 않고 Tab 키를 누르는 위치다. 후보를 완성하는 것만으로 명령이 실행되지는 않는다.

```text
bxdl st[Tab]                          → start, status, stop
bxdl setup [Tab]                      → new, resume, list, show, edit, apply 등
bxdl status lo[Tab]                    → 저장한 노드 이름 local
bxdl setup resume lo[Tab]              → 저장한 준비 이름 local
bxdl setup edit local --field http[Tab] → httpAddress, httpPort
bxdl status local --[Tab]              → 해당 명령의 옵션
bxdl status --home "/path/to/team home" lo[Tab]
```

`local`은 예시다. 실제 저장된 이름만 제안하며 `setup new`나 `nodes add`의 새 이름은 사용자가 정한다. `--home`이 있는 명령줄은 그 관리 위치의 이름을 사용하고 없으면 Mac의 `~/Library/Application Support/BXDL`을 사용한다. 다른 OS에서는 명시 `--home`을 사용한다. `--home`, `--instance`, 입력 파일 등의 경로 값에서는 zsh의 디렉터리/파일 완성을 사용한다.

후보 조회는 명령줄의 변수나 명령 치환을 평가하지 않는다. 예를 들어 `--home "$HOME/team"` 또는 `--home '$HOME/team'`의 `$HOME`은 후보 조회에서 실제 경로로 바꾸지 않는다. 명시 `--home`에는 절대경로나 이미 완성된 경로를 입력하면 같은 이름 목록을 조회할 수 있다. `$(...)`도 실행하지 않는다. `--home ~/team`처럼 따옴표 없는 별도 인자의 `~/` 접두어는 현재 HOME 경로로 안전하게 바꿔 조회한다. 따옴표 안의 `~/`, `--home=~/team`, `~다른사용자/`는 이 변환에 포함하지 않으므로 절대경로나 이미 완성된 경로를 사용한다.

## 새 터미널에도 적용하기

현재 셸에서 확인한 뒤 직접 관리하는 completion 디렉터리에 스크립트를 저장할 수 있다. 기존 `_bxdl`을 관리하고 있다면 교체할 파일을 먼저 확인한다.

```zsh
mkdir -p "$HOME/.zsh/completions"
bxdl completion zsh > "$HOME/.zsh/completions/_bxdl"
```

자신의 zsh 설정에서 기존 `compinit` 호출 **앞**에 다음 `fpath` 설정을 넣는다. 이미 초기화하는 설정이 있다면 `compinit`을 중복 추가하지 않는다.

```zsh
fpath=("$HOME/.zsh/completions" $fpath)
autoload -Uz compinit
compinit
```

그 뒤 새 터미널을 연다. 설정 프레임워크가 완성 함수를 캐시해 새 파일을 찾지 못하는 경우 현재 셸에서는 위 `source <(bxdl completion zsh)` 방식으로 바로 적용할 수 있다. BXDL은 `.zshrc`·PATH·completion 파일을 자동으로 쓰지 않는다. 관리 CLI를 업데이트한 뒤에는 저장한 `_bxdl`도 같은 명령으로 갱신한다.

## 제안하는 정보와 한도

명령·옵션·정해진 값은 CLI가 지원하는 문법을 바탕으로 제안한다. 노드 이름은 catalog에 저장한 명시 이름이며 표시명이나 engine instanceId가 아니다. `setup resume/edit/apply`는 `sessions/`의 준비 이름을, `setup show`는 `sessions/`와 `operations/`에 남은 이름을 합쳐 제안한다. 이름 규칙과 디렉터리 속성만 확인하며 준비 내용이나 공급 자료를 재검증하지 않는다. 심볼릭 링크나 요구한 private 디렉터리 조건을 만족하지 않는 이름은 제안하지 않는다. 제안된 이름은 노드가 실행 중이거나 준비 자료가 현재 유효하다는 뜻이 아니다. 실제 조회는 `status` 또는 `setup show`로 한다.

Tab 조회는 관리 폴더·잠금·기록을 만들거나 수정하지 않으며 control·JAR·JRE·서비스·네트워크를 호출하지 않는다. 이름 조회는 디스크 전체를 탐색하지 않는다. 관리 위치가 없거나 읽을 수 없으면 이름 후보 없이 끝난다. 경로 완성은 zsh가 현재 경로 주변의 파일/디렉터리를 읽는 일반 동작을 따른다.

후보가 나타나지 않으면 `command -v bxdl`과 `bxdl completion zsh --help`로 새 관리 CLI를 선택했는지 확인한다. 별도 관리 위치의 이름은 같은 명령줄에 `--home`을 입력한 뒤 완성한다. 잘못된 명령/옵션 조합이나 후보 조회 실패는 후보 없이 끝나며, 자동으로 다른 관리 위치를 찾아보거나 설정을 복구하지 않는다.

자동완성은 관리 CLI의 입력 편의 기능이다. 기존 RC.3 runtime package를 다시 만들지 않으며 NIGO/JRE·노드 DB·LaunchAgent 동작은 바꾸지 않는다. 독립 Mac 관리 CLI 배포, 새 CLI와 기존 runtime 조합의 서비스/네트워크 인수, 작성자가 아닌 사용자의 과업 검증은 [CLI-05 후속 계획](../design/2026-10-22-integrated-operations-ux-implementation-plan.md)에 따로 남긴다.
