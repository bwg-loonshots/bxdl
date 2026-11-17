# Linux JRE·엔진 cold 식별 검사

선정 Linux JRE와 RC.4 엔진의 식별 정보·모듈·파일 불변성을 실제 Linux에서 확인하는 기술 검사다. **DB 초기화·RocksDB native 실행·노드·systemd 서비스 인수는 포함하지 않는다.** 설치·초기화·운용까지 수행하려면 [Linux 설치와 운용](./linux-operations.md), 후보 제작은 [Linux 빌드 안내](./linux-native-build.md)를 따른다.

Ubuntu CI에서 BXDL CLI의 native 빌드와 자동 검사는 통과했다. 이 문서의 선정 JRE/RC.4 cold 검사는 별도이며 현재 실제 실행 결과는 NOT_RUN이다. 검사 담당자가 직접 실행하고 정제 JSON을 반환할 수 있으므로 SSH 주소·계정·인증정보를 공유할 필요는 없다.

## 1. 환경과 검증한 입력

대상은 **Ubuntu 24.04 LTS · x86_64 · glibc · PID 1 systemd 255 이상**, system manager `running`인 실제 Linux다. WSL2는 후속이다. 일반 사용자로 실행하며 Python 3와 검증한 native Linux CLI가 필요하다. 이미 빌드한 CLI를 검사할 때 Rust/Cargo는 필요 없으며, CLI 제작이 필요한 경우 [빌드 절차](./linux-native-build.md)를 먼저 수행한다. 이 검사는 sudo·계정 생성·unit 설치·서비스 변경을 수행하지 않는다.

다음 자료의 출처와 SHA-256을 전달물 밖의 검토한 기준으로 확인한다. 같은 폴더의 관측값만 다시 계산해 신뢰 기준으로 삼지 않는다.

- 검토한 source와 그 source로 만든 Linux `bxdl` 실행 파일. binary revision과 외부 CLI hash를 함께 고정한다.
- [Linux runtime lock](../packaging/runtime-lock.temurin-21-linux-amd64.json), [Linux RC.4 engine lock](../packaging/engine-lock.rc4-temurin-linux-development.json).
- 고정 JRE archive 또는 이미 검증·materialize한 flat runtime, 고정 RC.4 JAR.
- 같은 검토 source의 `tools/prepare-linux-runtime.py`, `tests/acceptance/linux_runtime_probe.py`.

배포물은 외부 archive checksum을 먼저 대조하고 새 전용 폴더에 푼다. 내부 `SHA256SUMS`는 손상 검사용이며 그 자체가 공급자의 신원을 인증하지 않는다. source·도구·공급물의 전달 기준은 `HANDOFF.json` 또는 해당 시험의 외부 고정 기록을 따른다. 과거 cold 전달본의 source 파일명을 최신 빌드에 그대로 사용하지 않는다.

입력 파일과 경로는 root 또는 호출 사용자 소유를 신뢰한다. 다른 사용자 소유 자료, symlink·hardlink·특수 파일과 타인이 쓸 수 있는 조상 경로는 거절한다. 신뢰하는 소유자의 sticky 공용 조상은 보호된 자식 경로에 한해 허용한다. 운영 DB·고객 키는 이 검사에 필요하지 않다.

## 2. 경로와 외부 pin 지정

아래는 검토한 BXDL source의 최상위 폴더에서 실행한다. 절대 경로와 외부에서 확인한 값을 직접 지정한다. 각 명령의 성공을 확인한 뒤 다음으로 진행한다.

```bash
BXDL_CLI='/absolute/verified/linux/bxdl'
BXDL_CLI_SHA256='<별도로 고정한 CLI SHA-256>'
BXDL_SOURCE_COMMIT='<CLI에 포함된 검토 source의 40자리 commit>'
BXDL_RUNTIME_LOCK='/absolute/verified/runtime-lock.temurin-21-linux-amd64.json'
BXDL_RUNTIME_LOCK_SHA256='<별도로 고정한 runtime lock SHA-256>'
BXDL_ENGINE_LOCK='/absolute/verified/engine-lock.rc4-temurin-linux-development.json'
BXDL_ENGINE_LOCK_SHA256='<별도로 고정한 engine lock SHA-256>'
BXDL_JAR='/absolute/verified/nigo-node.jar'
BXDL_CHECK_ROOT='/absolute/existing-parent/new-cold-check'
umask 077
mkdir -m 700 "$BXDL_CHECK_ROOT"
```

`BXDL_CLI`는 실행 가능 mode `0755`의 정규파일이어야 한다. 이 CLI의 hash는 해당 bytes를 묶는 값이며 소스 출처 검토나 배포 서명을 대신하지 않는다. `BXDL_CHECK_ROOT`는 새 폴더여야 하고 부모는 미리 존재해야 한다.

## 3. 실행 전 runtime 준비

이미 `prepare-linux-operations.py`로 검증·추출한 runtime이 있으면 그 flat 폴더를 `BXDL_RUNTIME_HOME`으로 지정하고 다음 절로 진행한다. 원 archive를 직접 풀어 legal symlink가 남은 폴더는 사용하지 않는다.

archive부터 준비하는 경우에는 다음을 사용한다.

```bash
python3 -B tools/prepare-linux-runtime.py \
  --archive /absolute/verified/jre.tar.gz \
  --runtime-lock "$BXDL_RUNTIME_LOCK" \
  --runtime-lock-sha256 "$BXDL_RUNTIME_LOCK_SHA256" \
  --output "$BXDL_CHECK_ROOT/prepared" \
  > "$BXDL_CHECK_ROOT/runtime-prepare-summary.json"
```

exit 0과 `outcome=SUCCEEDED`를 확인한 뒤 `BXDL_RUNTIME_HOME`을 정한다.

```bash
BXDL_RUNTIME_HOME="$BXDL_CHECK_ROOT/prepared/runtime"
```

선정 JRE는 257개 정규파일을 가지며 공급 legal symlink는 대상 내용으로 materialize한다. 준비 단계는 Java를 실행하지 않아 `runtimeExecution=NOT_RUN`이 정상이다. 준비 실패 시 이후 검사를 실행하지 않고 정제 요약을 보존한다.

## 4. 실제 Linux cold 검사

```bash
python3 -B tests/acceptance/linux_runtime_probe.py \
  --cli "$BXDL_CLI" \
  --cli-sha256 "$BXDL_CLI_SHA256" \
  --cli-source-commit "$BXDL_SOURCE_COMMIT" \
  --runtime-home "$BXDL_RUNTIME_HOME" \
  --runtime-lock "$BXDL_RUNTIME_LOCK" \
  --runtime-lock-sha256 "$BXDL_RUNTIME_LOCK_SHA256" \
  --engine-lock "$BXDL_ENGINE_LOCK" \
  --engine-lock-sha256 "$BXDL_ENGINE_LOCK_SHA256" \
  --jar "$BXDL_JAR" \
  --output "$BXDL_CHECK_ROOT/probe" \
  > "$BXDL_CHECK_ROOT/linux-probe-summary.json"
```

검사는 실제 host·CLI identity·전체 JRE inventory·Java properties/modules·RC.4 `engine-info`와 입력의 전후 불변성을 확인한다. systemd manager가 `degraded`이거나 입력이 다르면 기준을 통과하지 못한다. 통과시키려고 systemd 상태나 pin을 임의로 바꾸지 않는다.

- exit 0, `outcome=PASS`, `reasonCode=LINUX_COLD_IDENTITY_VERIFIED`: **cold 식별 범위만 통과**.
- exit 3: 정제 JSON의 `reasonCode` 확인. 초기 환경 검사에서 거절되면 `probe/result.json`이 없을 수 있으므로 stdout 요약을 사용한다.
- 기존 출력과 실패 자료는 보존한다. 재시도는 원인을 확인한 뒤 새 검사 폴더에서 수행한다.

## 5. 결과와 다음 검증

`probe/result.json`이 있으면 그 정제 결과를 반환한다. 없으면 `linux-probe-summary.json`, runtime 준비에서 중단했다면 `runtime-prepare-summary.json`을 반환한다. 원문 `probe/evidence/`와 작업 폴더 전체는 보내지 않는다. Java 원문에 HOME·계정·개인 경로가 포함될 수 있어 원자료는 해당 머신에 보관한다.

PASS여도 native provider·RocksDB·초기화·서비스·runtime package 인수는 NOT_RUN이다. 비공개 설치·운용 시험은 이 기술 검증과 사용자 경험을 구분해 실제 init·서비스·정상 종료·같은 DB 재시작까지 관측한다. 전체 범위는 [구현 상태](./implementation-status.md)와 [Linux 실행 계획](../design/2026-11-16-linux-first-implementation-plan.md)을 따른다.
