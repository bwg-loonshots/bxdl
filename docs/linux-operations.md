# Linux 설치와 운용

일반 사용자 CLI에서 설치를 준비하고, 관리자 승인으로 시스템 노드를 설치한 뒤 이름으로 초기화·운용하는 개발용 경로다. Ubuntu CI의 native CLI 빌드·자동 검사는 통과했으며, 선정 JRE/RC.4의 실제 설치·RocksDB·systemd·SSH 단절·DB 재시작 인수는 아직 NOT_RUN이다. 정식 signed Linux runtime/exact executor profile 인수도 별도다. 비공개 시험에서는 빌드를 사전 준비로 두고 아래 설치·초기화·운용까지 직접 검증한다.

[Linux 네이티브 빌드 안내](./linux-native-build.md)의 도구로 고정 후보를 만들고 `prepare-linux-operations.py`로 archive를 설치용 flat JRE/JAR·worker·독립 CLI로 연결한다. `prepare-linux-test-inputs.py`는 새 시험용 credential과 단일 노드 설정을 장비 안에서 만든다. source·도구·공급물 identity는 각 전달본의 `HANDOFF.json`을 따른다. 문서의 과거 검증 기록을 최신 전달본의 source로 자동 간주하지 않는다.

대상은 Ubuntu 24.04 LTS·x86_64/glibc·PID 1 systemd의 실제 Linux 머신이다. WSL2는 후속이다. Mac의 기존 설치·LaunchAgent는 그대로 유지하며 Mac에서 `setup new/resume/edit/apply --linux`를 실행하지 않는다. Mac의 `setup list/show --linux`는 저장 기록 조회 형식만 선택한다.

## 1. 준비할 자료와 CLI

일반 사용자 터미널에서 시작한다. `sudo bxdl setup`으로 사용자 home을 root 소유로 만들지 않는다. 준비 자료는 다음과 같다.

- [선정 Linux JRE](../packaging/runtime-lock.temurin-21-linux-amd64.json)의 flat runtime 폴더. [runtime 준비 도구](./linux-validation.md)의 `prepared/runtime` 형태이며, 원 archive를 임의로 풀어 legal symlink가 남은 폴더와 구분한다.
- [Linux engine lock](../packaging/engine-lock.rc4-temurin-linux-development.json)에 고정한 RC.4 JAR. Mac JRE·Mac 실행 파일은 사용할 수 없다.
- 같은 시험 노드를 위한 제품 `instance.json`, NIGO native `node.json`, 공통 체인 자료와 시험용 validator/TLS key·password **파일**. 비밀번호 원문을 문답이나 명령 인자로 입력하지 않는다.
- 현재 Linux setup·관리 프로토콜을 지원하는 검증된 Linux 관리 CLI. 새 고정 후보의 제작·회수에는 [native build kit](./linux-native-build.md)를 사용한다. 과거 cold 검사 전용 source와 현재 설치 도구를 섞지 않는다.

키/password 등 private 입력은 호출 사용자가 소유한 `0600` 정규파일로 준비한다. 입력 경로·상위 경로의 소유자/권한과 symlink를 검사하며, root helper가 읽을 수 있다는 이유로 root 전용 비밀을 대신 읽지 않는다. 설치 영역 내부를 공급물 경로로 지정하지 않는다. 사용자 원본은 수정하지 않고 승인한 자료를 시스템 영역에 별도 보관한다.

고정 후보는 [네이티브 빌드 안내](./linux-native-build.md)의 source·도구·JRE/JAR pin을 확인하고 일반 사용자로 제작한다. 제작 도구는 archive와 receipt를 만들며 sudo·계정 생성·설치·JVM·systemd를 수행하지 않는다. Linux 실행기 profile 인수는 별도 단계다.

개별 개발용 CLI를 직접 빌드한다면 검토한 source commit으로 이동해 다음처럼 확인한다. `git status --short`가 비어 있는지 확인한 뒤 진행한다. Rust 1.86.0·C 링커가 필요하며 Cargo 의존성 캐시가 없으면 네트워크가 필요하다. 이는 오프라인 최종 제품 설치 절차가 아니다.

```sh
git status --short
git rev-parse HEAD
BXDL_REVISION="$(git rev-parse HEAD)" cargo build --locked --release --target x86_64-unknown-linux-gnu
BXDL_BIN="$PWD/target/x86_64-unknown-linux-gnu/release/bxdl"
"$BXDL_BIN" version --json
"$BXDL_BIN" host check --json
```

각 명령이 성공했는지 확인한 뒤 다음 단계로 진행한다. `host check` 성공은 JRE/native·엔진·서비스 인수 성공이 아니다. 이 문서의 이후 `bxdl`은 이 검토한 Linux 실행 파일 또는 같은 후보로 설치한 관리 CLI 진입점을 뜻한다. CLI-only 배포·사용자 설치는 [관리 CLI 안내](./management-cli.md)를 따른다.

## 2. 관리자 최초 준비

시스템 설치 helper의 경로는 **`/usr/local/libexec/bxdl/bxdl`**로 고정한다. 관리자는 검토한 Linux executable을 이 위치에 사전 배치해야 한다. 실행 파일은 root 소유 정규파일·실행 가능·hardlink 없음이어야 하고, 모든 상위 경로는 root 소유이며 group/other 쓰기와 symlink가 없어야 하고 일반 사용자가 통과할 수 있어야 한다. 권장 실행 파일 mode는 `0755`다.

다음은 최초 배치를 위한 예시다. 관리자는 먼저 `namei`와 `stat`으로 기존 경로를 확인한다. **대상 파일 또는 symlink가 이미 있으면 이 예시를 중단하고 기존 helper의 출처·사용 여부를 확인한다.** 기존 helper를 이 문서가 자동 교체하지 않는다. 디렉터리 생성도 새 전용 위치에 한해 수행한다.

```sh
namei -l /usr/local/libexec/bxdl/bxdl
```

경로가 새 위치이고 상위 디렉터리가 위 조건에 맞는 것을 확인한 관리자가 다음 두 단계를 별도로 수행한다. `BXDL_BIN`은 앞서 검증한 native Linux 실행 파일이다.

```sh
sudo /bin/sh -eu -c '
  test ! -e /usr/local/libexec/bxdl/bxdl
  test ! -L /usr/local/libexec/bxdl/bxdl
  check_root_dir() {
    test -d "$1"
    test ! -L "$1"
    test "$(/usr/bin/stat -c %u -- "$1")" = 0
    bxdl_dir_mode=$(/usr/bin/stat -c %a -- "$1")
    test "$((0$bxdl_dir_mode & 0022))" = 0
    test "$((0$bxdl_dir_mode & 0001))" = 1
  }
  for bxdl_dir in / /usr /usr/local; do check_root_dir "$bxdl_dir"; done
  for bxdl_dir in /usr/local/libexec /usr/local/libexec/bxdl; do
    if test ! -e "$bxdl_dir" && test ! -L "$bxdl_dir"; then
      /usr/bin/mkdir -m 0755 -- "$bxdl_dir"
    fi
    check_root_dir "$bxdl_dir"
  done
'
```

기존 helper가 있으면 종료하는 아래 최초 배치 명령을 사용한다. 상위 경로가 root에 의해 보호된 상태에서만 수행한다.

```sh
sudo /bin/sh -eu -c '
  test ! -e /usr/local/libexec/bxdl/bxdl
  test ! -L /usr/local/libexec/bxdl/bxdl
  /usr/bin/install -o root -g root -m 0755 "$1" /usr/local/libexec/bxdl/bxdl
' bxdl-bootstrap "$BXDL_BIN"
```

이 단계는 helper 파일 배치다. 계정 생성·노드 설치·초기화·서비스 시작을 하지 않는다. 실제 노드의 전용 계정과 `/var/lib/bxdl` 제어 영역은 이후 사용자의 명시 `setup apply` 승인으로 준비한다. sudoers나 passwordless 정책을 자동 설정하지 않는다.

## 3. 설치 준비·수정·중단 후 재개

```sh
bxdl setup new local \
  --runtime /srv/bxdl-input/prepared/runtime \
  --jar /srv/bxdl-input/nigo-node-0.0.1-SNAPSHOT.jar \
  --native-config /srv/bxdl-input/node.json \
  --from /srv/bxdl-input/instance.json
```

플래그를 생략하면 필요한 자료와 제품 항목을 순서대로 묻는다. 최초 승인은 사용자 준비 기록 생성에 대한 승인이다. `:help`는 도움말, `:back`은 이전 항목, `:cancel`·EOF는 입력을 보존하고 중단한다. 이 단계는 JVM·DB·systemd를 실행하지 않는다. 마지막 검토 승인은 공급물 hash·설정 검증과 불변 설치 요청 저장이며 시스템 설치는 별도다.

```sh
bxdl setup list
bxdl setup show local
bxdl setup edit local --field httpPort
bxdl setup edit local --field runtimeRoot
bxdl setup resume local
```

Linux의 공급물 수정 필드는 `runtimeRoot`, `engineJar`, `nativeConfig`다. 제품 필드도 수정할 수 있지만 인스턴스 이름과 데이터 위치는 준비 이름으로 고정한다. 이름을 바꾸려면 다른 이름으로 새 준비를 만든다. 잘못된 값과 같은 값 재입력은 기존 검토를 보존한다. 실제 값을 바꾸면 과거 검토를 해제하고 `setup resume`에서 다시 검토한다. 같은 경로의 파일 내용이 변경된 경우에도 `setup resume`의 마지막 검토를 다시 승인하면 새 revision으로 해시를 고정한다. apply 시도 이후에는 입력을 동결하며 결과가 불명확한 계획을 덮거나 다른 공급물로 재시도하지 않는다.

`list/show --json`은 읽기 전용이다. `show`의 `reviewedInputs`는 준비 원자료의 현재 대조 결과이고 `installation`은 저장된 설치 상태다. 이 둘은 서비스가 현재 READY인지 나타내지 않는다. 읽기 명령은 계정·DB·서비스를 만들지 않는다.

## 4. 시스템 설치와 이름 연결

```sh
bxdl setup apply local
bxdl nodes
```

apply는 일반 사용자 TTY에서 대상 경로·전용 계정·설정과 개발용 공급물 범위를 보여주고 `y/yes` 승인 후 sudo 인증을 요청한다. 인증정보는 sudo가 직접 처리한다. 승인한 요청·hash·호출 UID만 root helper에 전달한다. root helper는 입력을 다시 대조해 별도 사본과 설치 거래 기록을 만들고, 전용 nologin 계정·root 보호 공급물·cold 검사·노드 등록을 진행한다. 성공 뒤 일반 사용자 프로세스가 자기 catalog에 이름을 연결한다.

| 저장 위치 | 소유자와 내용 |
| --- | --- |
| `$HOME/.local/share/BXDL-CLI` | 독립 관리 CLI를 설치한 경우의 사용자 배포 저장소 |
| `$HOME/.local/share/BXDL/sessions/local` | 사용자 준비 답·검토 revision·불변 요청·apply 시도/완료 요약 |
| `$HOME/.local/share/BXDL/catalog` | 사용자별 노드 이름·control identity·descriptor digest. root 설치 과정에서 쓰지 않음 |
| `/var/lib/bxdl/instances/local` | root 제어 기록·고정 runtime/JAR/worker/config, 공개 `management.json` |
| `/var/lib/bxdl/instances/local/data` 및 지정 output | 해당 노드 전용 UID의 DB·엔진 출력. root 제어 기록과 다른 노드는 쓰기 불가하도록 구성 |
| `/var/lib/bxdl` 아래 설치 거래 영역 | root 설치 시도·승인 입력 사본·계정 준비와 완료/불명 결과 |

`--home <절대 경로>`를 썼다면 이후 이름 기반 명령에도 같은 `--home`을 사용한다. 사용자 home을 시스템 control 안에 두지 않는다. `nodes`의 `SYSTEM_REGISTERED`는 등록 기록만 의미하며 현재 초기화·실행 상태는 `NOT_OBSERVED`다.

완료된 동일 apply를 다시 호출하면 완료 근거와 이름 연결을 확인한다. 부분 실패·UNKNOWN이면 기존 폴더·계정·자료를 보존하고 원인을 확인한다. 실패한 계획의 기록을 지우거나 기존 control을 빈 설치 대상으로 바꾸지 않는다. apply는 init·start를 포함하지 않는다.

## 5. 초기화·운용과 독립 CLI

명령을 한 줄씩 실행하고 결과를 확인한 뒤 다음 단계로 진행한다. init·stop 실패 또는 UNKNOWN이면 다음 start를 실행하지 않는다.

```sh
sudo -v
bxdl init local --confirm-initialize
bxdl start local
bxdl status local --wait
bxdl logs local
bxdl diagnose local --output ./local-diagnostic.json
bxdl stop local
```

일반 운용 명령은 터미널 여부와 관계없이 기존 sudo 인증만 사용하며 `sudo -n`으로 고정 worker를 호출한다. 인증이 없거나 만료되면 거부하고 `sudo -v` 후 같은 명령을 다시 실행하도록 안내한다. 비밀번호 문답을 운용 명령의 stdin에서 읽지 않는다. 위 `sudo -v`는 인증만 준비하고 노드를 변경하지 않는다. `setup apply`의 명시 설치 문답은 승인 뒤 sudo 인증을 직접 요청한다. 명령 전체를 sudo로 실행해 사용자의 이름 목록을 다른 root home에서 찾게 만들지 않는다. `resume-init local --confirm-resume`은 기존 미완료 초기화의 동일 identity·잠금 조건을 충족할 때만 사용한다.

일반 관리 CLI는 공개 `management.json`의 root 소유·control device/inode·protocol과 최초 worker hash/size를 확인한다. 이름 목록의 descriptor digest와도 대조한 뒤 **등록 때 보관한 고정 worker**에 정해진 요청만 전달한다. 관리 CLI 업데이트가 기존 worker/JAR/JRE·DB를 바꾸지 않는다. 이 개발용 프로토콜의 분리 구현은 정식 Linux exact profile이나 실제 CLI 전환 인수 완료를 뜻하지 않는다.

설치 직후 서비스 NOT_STARTED는 exit 5일 수 있다. init 성공은 INITIALIZED, start/status의 목표는 로컬 READY, 정상 stop은 INACTIVE/STOPPED와 `LINUX_SERVICE_STOPPED_VERIFIED`다. `start` 응답·systemd active와 노드의 로컬 READY, 네트워크 정족수·거래 확정은 별개다. 종료는 해당 시도의 엔진 closure·OS 종료·process/cgroup 부재·DB lock 근거를 대조한다. SSH 연결 해제 뒤 계속 동작하는지와 실제 계정 격리는 Linux 실물 인수에서 확인해야 한다. 자동 재시작·부팅 enable·재부팅 복구를 이번 구현의 완료 기능으로 계산하지 않는다.

`logs/diagnose`는 제한된 저장 기록·초기화/실행 보고서를 정제한다. raw stdout/stderr·system journal·비밀을 그대로 내보내지 않고, 누락·변조·예산 초과는 `partial`/`INCOMPLETE`로 남긴다. 현재 상태를 확인하려고 새 HTTP/systemctl/JVM을 실행하는 검사는 아니다. diagnose는 control 밖의 **기존에 없는** 출력 파일을 호출 사용자 `0600`으로 생성한다. 파일 생성 성공이 노드 정상 운용 판정은 아니다.

## 6. 실제 검증과 기존 등록의 호환

LNX-03의 오래된 worker/control에는 새 공개 descriptor 프로토콜이 없을 수 있다. 현재 관리 CLI는 이를 자동 채택하거나 worker를 교체하지 않는다. 기존 개발 기록은 원래 실행기로 다루고, 현재 UX는 새 시험 이름·새 control로 검증한다. 새 저수준 `linux-service register`와 옛 기록의 호환 경계는 [개발 backend 안내](./linux-service-development.md)를 따른다.

비공개 전달본은 빌드·후보 고정과 설치 입력 준비 이후 위 설치·init·서비스·정상 종료·DB 재시작·SSH 단절 과업을 수행하도록 구성한다. 결과/개선 의견 양식은 Git 밖에서 제공하며 기술 결과와 사용 편의성을 나누어 관측한다. CLI 버전 전환·다중 노드·장기 운영·WSL2는 후속이다. 제작·수신·CI 성공만으로 Linux runtime/executor profile을 승인하지 않는다. 실제 조합의 진행 상태는 [구현 상태](./implementation-status.md), 남은 범위는 [실행 계획](../design/2026-11-16-linux-first-implementation-plan.md)을 따른다.
