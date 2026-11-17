# Linux 네이티브 빌드와 개발 패키지 고정

2026-11-17: **빌드 입력 키트 준비·오프라인 검증 완료, 실제 Linux 빌드와 패키지 해시 확정은 대기**다. 이 작업 환경은 Mac arm64이며 사용 가능한 Linux 빌더가 없다. SSH 접속 정보를 공유할 필요는 없다. Ubuntu 장비 사용자가 아래 빌드 명령을 직접 실행하고 결과를 전달할 수 있다.

최신 테스터 과업은 [UAT 안내](../UAT/README.md)를 따른다. 아래 `ba4d5af`와 2026-11-17 plan 사본은 최초 준비 이력이며, 새 전달본은 머지된 commit으로 재생성하고 `UAT/deliveries/<commit>/HANDOFF.json`에 source·plan·archive 해시를 기록한다. 생성물은 Git에 넣지 않는다.

## 1. 고정한 입력과 산출물의 구분

이번 키트의 제품 source는 LNX-04 commit `ba4d5af6b505ea880be034b73094ac8c92018d57`이다. dirty checkout을 복사하지 않고 그 commit을 `git archive`로 내보낸다. `Cargo.lock`, Rust 1.86.0 toolchain, Linux runtime/engine lock도 같은 commit에서 읽는다. 네이티브 빌드 도구 두 파일은 별도 SHA-256으로 고정한다. 이번 후속 작업의 CLI 오류 코드 보완·Linux 테스트 분리는 이 source commit에 포함되지 않는다. 해당 변경을 포함하는 후속 후보는 커밋 후 새 입력 키트를 만들어야 한다.

| 대상 | 현재 고정 상태 |
| --- | --- |
| Linux JRE | Temurin 21.0.13+9-LTS, Linux amd64/glibc; 기존 선정 archive·Java·257개 inventory 유지 |
| NIGO | 기존 RC.4 JAR·source·contract fingerprint 유지 |
| 제품 source | 위 LNX-04 commit, Git archive·Cargo.lock·toolchain 해시 |
| 빌드 도구 | build-linux-candidate.py·linux_build_contract.py의 별도 해시 |
| Linux 실행기 A / 관리 CLI C | 실제 native build 후 바이너리 SHA-256 확정. 초기 후보는 동일 bytes, 역할과 archive는 분리 |
| runtime R-L / CLI bundle | 실제 조립 후 archive·manifest SHA-256과 payload inventory 확정 |
| 인수·신뢰 | unsigned development 전용. 서명된 제품 배포·Linux 실행기 profile 인수는 별도 |

[준비 기록](../results/2026-11-17-linux-native-build-preparation.md)과 [입력 계획 사본](../packaging/linux-native-build-plan-20261117.json)을 함께 본다. 과거 LNX-02 cold probe 전달본은 변경하지 않는다. 입력의 해시 고정은 빌드 재현성·공급자 서명·엔진 정상 실행의 증명이 아니다.

## 2. 준비 담당자가 키트를 만드는 방법

이미 검증한 JRE archive와 RC.4 JAR를 사용한다. 새 출력 경로를 지정하며 기존 키트나 실패 결과는 덮어쓰지 않는다. Mac의 `/tmp`·`/var` 같은 symlink 별칭 대신 `/private/tmp` 또는 실제 경로를 쓴다. 출력의 부모 디렉터리는 미리 존재해야 한다.

```sh
python3 -B tools/prepare-linux-build.py \
  --source-commit ba4d5af6b505ea880be034b73094ac8c92018d57 \
  --runtime-archive /absolute/received/jre.tar.gz \
  --jar /absolute/received/nigo-node.jar \
  --output /private/tmp/bxdl-linux-inputs \
  --archive-output /absolute/output/bxdl-linux-build-kit.tar.gz \
  --allow-unsigned-development
```

도구는 공급물 SHA-256을 기존 lock과 대조한다. 관측한 값을 새 정답으로 덮어쓰지 않는다. `build-plan.json`과 그 외 7개 파일을 고정하고 선택적으로 전달 archive를 만든다. stdout의 `planSha256`과 `kitArchive.sha256`을 수신자가 신뢰하는 별도 전달 경로로 공유한다. 키트 안에서 읽은 해시를 그대로 정답으로 쓰지 않는다.

## 3. Linux 담당자의 빌드

필요 환경은 **실제 Ubuntu 24.04, x86_64, glibc, 일반 사용자**다. WSL2·다른 OS·root 실행은 이번 도구가 거절한다. 설치된 Python 3, rustup으로 설치한 Rust/Cargo 1.86.0과 PATH의 rustup proxy 명령, `/usr/bin/cc`, binutils의 `/usr/bin/readelf`가 필요하다. Cargo dependency cache가 없다면 crates 수신 네트워크도 필요하다. Cargo 사용자·상위 폴더 config가 있으면 별도 깨끗한 빌드 환경을 마련한다. 도구가 ambient linker·Rust flags·wrapper 설정을 승계하지 않는다. 빌드에는 systemd·sudo가 필요하지 않다.

공급받은 archive SHA-256을 먼저 대조한 뒤, 새 전용 폴더에 일반 사용자로 푼다. 아래 `EXPECTED_*` 값은 준비 담당자에게 별도로 받은 값으로 지정한다. `DELIVERY`는 기존 경로와 겹치지 않는 새 경로다.

```sh
KIT_ARCHIVE=/absolute/received/bxdl-linux-build-kit.tar.gz
EXPECTED_KIT_SHA256='<별도로 받은 archive SHA-256>'
EXPECTED_PLAN_SHA256='<별도로 받은 plan SHA-256>'
DELIVERY="$HOME/bxdl-linux-build-delivery-01"
sha256sum "$KIT_ARCHIVE"
# 출력이 EXPECTED_KIT_SHA256과 정확히 같을 때만 다음 명령을 진행한다.
mkdir -m 700 "$DELIVERY"
tar --no-same-owner --same-permissions -xzf "$KIT_ARCHIVE" -C "$DELIVERY"
KIT="$DELIVERY/bxdl-linux-build"
python3 -B "$KIT/tools/build-linux-candidate.py" \
  --kit "$KIT" --plan-sha256 "$EXPECTED_PLAN_SHA256" \
  --output "$DELIVERY/candidate" --allow-unsigned-development
```

`PASS`와 종료 코드 0일 때만 조립 성공이다. 도구는 source commit·모든 입력·도구 자체를 대조하고 새 target에서 native release를 빌드한다. 생성한 실행 파일은 ELF64 little-endian x86_64·glibc 요구 버전을 검사한 뒤 `version`, package/CLI bundle build·verify에만 실행한다. `runtime/bin/java`와 전체 JRE의 ELF도 요구 버전을 검사하지만 Java·노드를 실행하지 않는다.

현재 manifest의 `minGlibc`는 2.34다. ELF의 `VERSION_NEEDS`에서 더 높은 요구가 나오면 `GLIBC_REQUIREMENT_EXCEEDS_DECLARATION`으로 중단한다. 이 검사를 건너뛰거나 숫자만 바꾸지 않고 빌더/sysroot와 지원 정책을 다시 결정한다. 이 검사를 통과해도 모든 native dependency 또는 Ubuntu 이전 버전의 실행 호환성을 인수한 것은 아니다. JAR 내부 RocksDB native는 후속 실행 검증 대상이다.

성공 시 다음 세 파일을 반환한다.

- `candidate/runtime.tar.gz`: 고정 실행기 A·JRE·RC.4 JAR·선정 JRE notices/SBOM을 담은 개발용 runtime 후보
- `candidate/management-cli.tar.gz`: 독립 관리 CLI C·자동완성·문서 bundle
- `candidate/receipt.json`: source·toolchain·host·ELF 관측·각 바이너리·archive·manifest 해시와 실행 범위

`sha256sum "$DELIVERY/candidate/receipt.json"` 결과도 별도로 전달한다. `candidate/work/`는 실패 원인 분석용 로컬 작업·원문 로그를 포함하므로 자동 공유하지 않는다. 실패하면 성공 후보로 취급하지 않고 결과를 보존한 채 새 출력 경로에서 재시도한다. 도구는 실패한 결과나 기존 경로를 자동 삭제하지 않는다. 제품 전체의 라이선스/SBOM 완결성은 후속 gate다.

## 4. Mac 등 수신 측의 검증

수신한 Linux 실행 파일·JRE를 실행하지 않는다. 수신자가 이미 검증한 **로컬 OS용 BXDL CLI**와 그 해시를 지정한다. 현재 checkout에서 준비한 `receive-linux-candidate.py`와 `linux_build_contract.py`를 사용한다. 반환 세 파일은 한 폴더에 둔다.

```sh
python3 -B tools/receive-linux-candidate.py \
  --kit /absolute/original-pinned-kit \
  --plan-sha256 '<별도로 고정한 plan SHA-256>' \
  --received /absolute/received-candidate \
  --receipt-sha256 '<별도로 받은 receipt SHA-256>' \
  --cli /absolute/trusted-local-bxdl \
  --cli-sha256 '<로컬 verifier SHA-256>'
```

로컬 CLI의 읽기 전용 archive verifier로 두 archive 전체를 검사하고 manifest·source·JAR·JRE 전체 inventory·A/C/Java 해시를 대조한다. ELF header는 bytes로 읽는다. 결과 `VERIFIED`는 **고정 입력에 대한 오프라인 수신 검증**이다. receipt의 native build 성공은 빌더가 보고한 값이며 원격 실행 증명은 아니다. systemd·engine native·실행기 profile 상태는 자동으로 PASS로 바뀌지 않는다.

## 5. 패키지 고정 후 다음 단계

현재 Linux `setup`은 flat JRE/JAR 경로를 입력받고 관리자 helper를 고정 worker로 snapshot한다. generic `bxdl install`의 runtime archive 설치 경로는 아직 Mac 전용이다. 이 빌드 도구로 R-L archive를 만들었다고 Linux 일반 설치·profile 인수가 완료되는 것은 아니다. 다음 Linux 설치 시험의 root helper는 수신 검증한 실행기 A bytes에서 배치하고 해시를 확인해야 한다. 자세한 권한·수동 bootstrap은 [Linux 설치·운용](./linux-operations.md)을 따른다.

후속 순서는 실제 빌드 결과 수신·조합 해시 확정 → 선정 JRE의 cold/engine-info·native/RocksDB 확인 → 새 Linux 설치·init·systemd 수동 시작/정상 종료 → SSH 단절 유지·동일 DB 재시작·독립 관리 CLI 전환이다. 각 결과를 확보한 뒤에만 exact executor profile과 테스터 배포본을 인수한다. 현재 새로운 NIGO 선행 구현 요청은 없다. Linux 실행에서 엔진/native 문제가 발견되면 고정된 RC.4 조합과 재현 자료로 별도 요청한다.
