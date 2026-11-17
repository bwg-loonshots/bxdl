# LNX-02 Linux 기술 검증 전달 안내

테스터가 Linux 머신에서 아래 도구를 직접 실행하고 **정제 결과 JSON만 전달**한다. SSH 주소·계정·비밀번호·개인키를 공유할 필요는 없다. 이는 Linux JRE와 RC.4의 식별·구성 확인 단계이며, 최종 사용자 설치·운용 UX 시험은 이후 LNX-04/05에서 진행한다.

## 1. 대상과 전달물 확인

대상은 **Ubuntu 24.04 LTS·x86_64·glibc·PID 1 systemd**이며 system manager가 `running`이어야 한다. WSL2는 후속 대상이다. 일반 사용자로 실행하며 Python 3, Rust toolchain 1.86.0, C 링커가 필요하다. 이 절차에는 sudo나 서비스 설정 변경이 없다. 입력 파일과 경로는 root 또는 호출 사용자 소유를 신뢰하며, 다른 사용자 소유 자료나 타인이 쓸 수 있는 조상 경로는 거절한다. root/호출 사용자 소유의 sticky 공용 조상(예: `/tmp`)은 보호된 자식 경로에 한해 허용한다.

전달자는 인계 archive와 **별도 경로에서 확인할 수 있는 archive SHA-256**을 함께 제공한다. 먼저 외부에서 확인한 값과 다운로드 파일의 `sha256sum` 결과를 대조한다. 일치한 archive를 새 전용 폴더에 해제하고 그 폴더로 이동한다. archive 안의 `SHA256SUMS`는 구성 파일의 무결성을 확인하는 자료이며, 그 파일만으로 공급자 신원을 인증하는 것은 아니다.

인계 폴더에는 다음 자료가 있어야 한다.

- `tools/prepare-linux-runtime.py`, `tests/acceptance/linux_runtime_probe.py`
- `packaging/runtime-lock.temurin-21-linux-amd64.json`, `packaging/engine-lock.rc4-temurin-linux-development.json`, `packaging/linux-rc4-candidate.json`
- `source/bxdl-source-9a8593638c7c.tar.gz` — 해제 후 최상위 폴더는 `bxdl-source/`
- `supply/OpenJDK21U-jre_x64_linux_hotspot_21.0.13_9.tar.gz`, `supply/nigo-node-0.0.1-SNAPSHOT.jar`와 대응 JRE source/build 자료
- `SHA256SUMS`

아래 명령은 인계 폴더 최상위에서 실행한다. 체크섬 불일치가 있으면 이후 단계로 넘어가지 않는다.

```bash
sha256sum -c SHA256SUMS
```

다음 값도 전달물과 독립적으로 검토한 저장소의 [Linux 후보](../packaging/linux-rc4-candidate.json)와 대조한다. 같은 인계 폴더의 파일에서 해시를 새로 계산하는 것만으로 신뢰 기준을 정하지 않는다. 후보가 변경되면 이 안내의 pin과 전달물을 함께 다시 확인한다.

```bash
BXDL_CANDIDATE_SHA256=a2e23d3575d1f7f178b9fb71f8f4ac86310b0b9685dfb24990bd6d8faab89b71
BXDL_RUNTIME_LOCK_SHA256=23eec871655dea8af8f79cb69f21c4add762226a9314df003edce1233e898f47
BXDL_ENGINE_LOCK_SHA256=9817d950f19832c725bfd373829023d44a437ad93e9f6291feef6c3185e94675
BXDL_SOURCE_COMMIT=9a8593638c7cb66f4793b9ae831b515a6060957d
printf '%s  %s\n' "$BXDL_CANDIDATE_SHA256" packaging/linux-rc4-candidate.json | sha256sum -c -
```

## 2. 고정 소스에서 Linux CLI 빌드

각 블록이 성공했는지 확인하고 다음 블록을 실행한다. 기존 `work`가 있으면 재사용하지 말고 새 인계 폴더에서 시작한다.

```bash
umask 077
BXDL_KIT_ROOT="$(pwd -P)"
mkdir -m 700 work
tar -xzf source/bxdl-source-9a8593638c7c.tar.gz -C work
```

다음 명령은 고정 소스에서 검증용 Linux CLI를 빌드한다. Cargo 의존성이 캐시에 없으면 네트워크가 필요하다. 현재 전달물은 오프라인 설치가 완성된 제품 패키지가 아니다.

```bash
BXDL_REVISION="$BXDL_SOURCE_COMMIT" \
CARGO_TARGET_DIR="$BXDL_KIT_ROOT/work/target" \
cargo +1.86.0 build --locked --release \
  --target x86_64-unknown-linux-gnu \
  --manifest-path "$BXDL_KIT_ROOT/work/bxdl-source/Cargo.toml"
```

빌드 성공 후 실행 파일 mode와 이번 실행에서 사용할 binary hash를 정한다. 이 해시는 이번에 빌드한 bytes를 묶는 값이며, 관리 CLI의 정식 배포 서명이나 소스 출처 검증을 대신하지 않는다.

```bash
BXDL_CLI="$BXDL_KIT_ROOT/work/target/x86_64-unknown-linux-gnu/release/bxdl"
chmod 0755 "$BXDL_CLI"
BXDL_CLI_SHA256="$(sha256sum "$BXDL_CLI" | cut -d ' ' -f1)"
```

## 3. JRE를 실행하지 않고 준비

```bash
python3 -B tools/prepare-linux-runtime.py \
  --archive "$BXDL_KIT_ROOT/supply/OpenJDK21U-jre_x64_linux_hotspot_21.0.13_9.tar.gz" \
  --runtime-lock "$BXDL_KIT_ROOT/packaging/runtime-lock.temurin-21-linux-amd64.json" \
  --runtime-lock-sha256 "$BXDL_RUNTIME_LOCK_SHA256" \
  --output "$BXDL_KIT_ROOT/work/prepared" \
  > work/runtime-prepare-summary.json
BXDL_PREPARE_EXIT=$?
printf 'prepare exit: %s\n' "$BXDL_PREPARE_EXIT"
cat work/runtime-prepare-summary.json
```

exit `0`, `outcome=SUCCEEDED`를 확인한다. 이때 `work/prepared/runtime/`에 257개의 정규파일이 준비된다. 145개의 공급 legal symlink는 대상 파일의 내용으로 복사된다. 준비 도구는 Java를 실행하지 않으므로 `runtimeExecution=NOT_RUN`이 정상이다. 실패했다면 다음 단계로 넘어가지 않고 `runtime-prepare-summary.json`을 전달한다.

## 4. 실제 Linux에서 cold 식별 검사

```bash
python3 -B tests/acceptance/linux_runtime_probe.py \
  --cli "$BXDL_CLI" \
  --cli-sha256 "$BXDL_CLI_SHA256" \
  --cli-source-commit "$BXDL_SOURCE_COMMIT" \
  --runtime-home "$BXDL_KIT_ROOT/work/prepared/runtime" \
  --runtime-lock "$BXDL_KIT_ROOT/packaging/runtime-lock.temurin-21-linux-amd64.json" \
  --runtime-lock-sha256 "$BXDL_RUNTIME_LOCK_SHA256" \
  --engine-lock "$BXDL_KIT_ROOT/packaging/engine-lock.rc4-temurin-linux-development.json" \
  --engine-lock-sha256 "$BXDL_ENGINE_LOCK_SHA256" \
  --jar "$BXDL_KIT_ROOT/supply/nigo-node-0.0.1-SNAPSHOT.jar" \
  --output "$BXDL_KIT_ROOT/work/probe" \
  > work/linux-probe-summary.json
BXDL_PROBE_EXIT=$?
printf 'probe exit: %s\n' "$BXDL_PROBE_EXIT"
cat work/linux-probe-summary.json
```

검사는 실제 host·CLI identity·전체 JRE inventory·Java properties/modules·RC.4 `engine-info`와 입력 파일의 전후 불변성을 확인한다. 환경이 다르거나 systemd manager가 `degraded`이면 정해진 기준을 통과하지 못할 수 있다. 통과시키기 위해 임의로 systemd 상태나 입력 pin을 바꾸지 말고 정제 결과를 전달한다.

- exit `0`, `outcome=PASS`, `reasonCode=LINUX_COLD_IDENTITY_VERIFIED`이면 이 cold 식별 범위를 통과했다.
- exit `3`이면 정제 JSON의 `reasonCode`를 확인한다. 초기 host 조건에서 거절되면 `work/probe/result.json`이 생성되지 않을 수 있으며 stdout을 저장한 `work/linux-probe-summary.json`으로 이유를 전달할 수 있다.
- 기존 출력은 덮어쓰지 않는다. 실패한 새 출력도 자동 삭제하지 않는다. 원인을 확인한 뒤 새 인계 폴더에서 별도 시도로 실행한다.

## 5. 전달할 결과와 남은 검증

`work/probe/result.json`이 있으면 그 파일을 전달한다. 없으면 `work/linux-probe-summary.json`을 전달한다. 두 파일은 정제 결과이므로 같은 실행의 둘 다 보낼 필요는 없다. JRE 준비 단계에서 실패했다면 `work/runtime-prepare-summary.json`만 전달한다.

`work/probe/evidence/`의 원문 stdout/stderr와 작업 폴더 전체는 보내지 않는다. Java 원문에는 HOME·계정·로컬 경로가 포함될 수 있으며 원자료는 해당 머신에 보관한다. 별도 검토가 필요할 때 공유 범위를 다시 정한다.

이번 도구의 `PASS`도 **native provider·RocksDB·DB 초기화·서비스·제품 패키지 인수는 NOT_RUN**이다. systemd는 manager 상태를 조회할 뿐 unit을 설치·시작·중지하지 않는다. 실제 Linux에서 이 도구를 아직 실행하지 않았다면 cold 검사 역시 NOT_RUN으로 남긴다. 후속 native/RocksDB 조합 검증, systemd 구현과 단일노드 인수 뒤에 테스터의 설치·운용 UX 과업을 진행한다. 전체 순서는 [Linux 구현 계획](../design/2026-11-16-linux-first-implementation-plan.md)을 따른다.
