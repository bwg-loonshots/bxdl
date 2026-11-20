# Linux 네이티브 빌드와 개발 패키지 고정

고정 source·Linux JRE·RC.4 JAR로 개발용 runtime과 독립 관리 CLI를 제작하고 검증하는 절차다. Ubuntu CI의 native CLI 빌드·자동 검사는 통과했다. 이슈 #27 수정 키트(소스 `954a98423b2d`)는 Ubuntu 24.04.5 x86_64 VM에서 고정된 NIGO RC.4·Temurin JRE로 두 패키지를 만들고 시험 입력 생성까지 `PREPARED`를 확인했다. CLI의 최대 요구 심볼은 `GLIBC_2.34`였다. 후속 Ubuntu 24.04.4 실기에서 같은 개발 후보의 노드 설치·초기화·systemd 운용·SSH 재접속과 재기동까지 확인했다. Ubuntu 장비 사용자가 직접 진행하므로 개발자에게 SSH 접속 정보를 제공할 필요는 없다.

테스터 검증은 Git 저장소와 분리한 비공개 전달 패키지로 진행한다. 안내서·빈 결과/개선 의견 양식·개별 실행 기록은 저장소에 보관하지 않는다. 전달본의 `HANDOFF.json`에 제품 source commit과 지원 도구·공급물 해시를 기록하며, 빌드는 사전 준비이고 본 과업은 노드 설치·초기화·서비스 운용 검증이다.

## 1. 고정한 입력과 산출물의 구분

키트의 제품 source는 전달 담당자가 검토한 완전한 commit SHA로 고정한다. dirty checkout을 복사하지 않고 그 commit을 `git archive`로 내보내며 `Cargo.lock`, Rust 1.86.0 toolchain, Linux runtime/engine lock도 같은 commit에서 읽는다. 제품 source와 별도로 네이티브 빌드 도구 두 파일의 SHA-256을 기록한다. 실제 전달본의 기준은 `HANDOFF.json`과 `build-plan.json`이며, 소스나 도구를 변경하면 새 키트와 외부 checksum을 발행한다.

| 대상 | 현재 고정 상태 |
| --- | --- |
| Linux JRE | Temurin 21.0.13+9-LTS, Linux amd64/glibc; 기존 선정 archive·Java·257개 inventory 유지 |
| NIGO | 기존 RC.4 JAR·source·contract fingerprint 유지 |
| 제품 source | 각 전달본이 지정한 검토된 commit, Git archive·Cargo.lock·toolchain 해시 |
| 빌드 도구 | build-linux-candidate.py·linux_build_contract.py의 별도 해시 |
| 비공개 재전달 키트의 링커 | Zig 0.14.1 Linux x86_64 원본 archive. SHA-256·크기를 plan의 `toolchain.linker`로 고정하며 CLI를 빌드할 때만 사용 |
| Linux 실행기 A / 관리 CLI C | 실제 native build 후 바이너리 SHA-256 확정. 초기 후보는 동일 bytes, 역할과 archive는 분리 |
| runtime R-L / CLI bundle | 실제 조립 후 archive·manifest SHA-256과 payload inventory 확정 |
| 인수·신뢰 | unsigned development 전용. 서명된 제품 배포·Linux 실행기 profile 인수는 별도 |

[최초 준비 기록](../results/2026-11-17-linux-native-build-preparation.md)과 [당시 입력 계획 사본](../packaging/linux-native-build-plan-20261117.json)은 과거 입력의 근거다. 현재 전달본의 source나 해시를 지정하는 파일로 사용하지 않는다. 입력의 해시 고정은 빌드 재현성·공급자 서명·엔진 정상 실행의 증명이 아니다.

## 2. 준비 담당자가 키트를 만드는 방법

검증한 JRE archive와 RC.4 JAR를 사용한다. 이슈 #27 재전달본에는 Zig 0.14.1의 Linux x86_64 원본 archive를 함께 넣는다. 준비 도구가 49,086,504바이트와 SHA-256 `24aeeec8af16c381934a6cd7d95c807a8cb2cf7df9fa40d359aa884195c4716c`을 검사한다. [Zig 공식 배포 목록](https://ziglang.org/download/index.json)

새 출력 경로를 지정하며 기존 키트나 실패 결과는 덮어쓰지 않는다. Mac의 `/tmp`·`/var` 같은 symlink 별칭 대신 `/private/tmp` 또는 실제 경로를 쓴다. 출력의 부모 디렉터리는 미리 존재해야 한다.

```sh
SOURCE_COMMIT='<검토한 40자리 commit SHA>'
python3 -B tools/prepare-linux-build.py \
  --source-commit "$SOURCE_COMMIT" \
  --runtime-archive /absolute/received/jre.tar.gz \
  --jar /absolute/received/nigo-node.jar \
  --linker-archive /absolute/received/zig-x86_64-linux-0.14.1.tar.xz \
  --output /private/tmp/bxdl-linux-inputs \
  --archive-output /absolute/output/bxdl-linux-build-kit.tar.gz \
  --allow-unsigned-development
```

도구는 공급물 SHA-256을 기존 lock과 대조한다. 관측한 값을 새 정답으로 덮어쓰지 않는다. `--linker-archive`를 지정한 키트는 `build-plan.json`과 8개 파일을 담는다. 이 옵션을 생략한 기존 형식은 7개 파일과 호스트 링커를 사용한다. 두 형식 모두 수신 시 plan과 실제 파일 목록을 대조한다. stdout의 `planSha256`과 `kitArchive.sha256`을 수신자가 신뢰하는 별도 전달 경로로 공유한다. 키트 안에서 읽은 해시를 그대로 정답으로 쓰지 않는다.

## 3. Linux 담당자의 빌드

필요 환경은 **실제 Ubuntu 24.04, x86_64, glibc, 일반 사용자**다. WSL2·다른 OS·root 실행은 이번 도구가 거절한다. 설치된 Python 3, rustup으로 설치한 Rust/Cargo 1.86.0과 PATH의 rustup proxy 명령, `/usr/bin/cc`, binutils의 `/usr/bin/readelf`가 필요하다. Cargo dependency cache가 없다면 crates 수신 네트워크도 필요하다. Cargo 사용자·상위 폴더 config가 있으면 별도 깨끗한 빌드 환경을 마련한다. 도구가 ambient linker·Rust flags·wrapper 설정을 승계하지 않는다. 빌드에는 systemd·sudo가 필요하지 않다.

비공개 시험 ZIP을 받았다면 새 폴더에 풀고 동봉된 `START-HERE.md`와 `00-PREPARE.md`를 따른다. 새 ZIP의 `HANDOFF.json`과 별도로 전달받은 해시로 새 `RUN`을 시작하고, 이전 시도의 실패 영수증과 로그는 원래 폴더에 보존한다. `prepare-test.py`는 빌드·패키지 검증·시험 입력 준비까지 성공하면 `PREPARED`를 기록한다. 설치·초기화·서비스 시험은 그 뒤에 진행한다. [테스터 보고 #27](https://github.com/bwg-loonshots/bxdl/issues/27)

빌드 키트만 수신했다면 archive SHA-256을 먼저 대조한 뒤, 새 전용 폴더에 일반 사용자로 푼다. 아래 `EXPECTED_*` 값은 준비 담당자에게 별도로 받은 값으로 지정한다. `DELIVERY`는 기존 경로와 겹치지 않는 새 경로다.

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

`PASS`와 종료 코드 0일 때만 조립 성공이다. 도구는 source commit·모든 입력·도구 자체를 대조하고 새 target에서 native release를 빌드한다. Cargo가 `release/deps`의 동일 실행 파일을 하드링크로 제공하면, 빌더는 원본의 소유자·권한·링크 위치를 확인하고 독립 파일로 복사한다. 복사 전후 내용과 크기를 대조하며 Cargo 원본의 권한은 보존한다. 실패하면 영수증과 표준 출력의 `reasonCode` 및 `message`, 표준 오류에서 Cargo 산출물과 거절 조건을 확인할 수 있다. 예를 들어 `CARGO_OUTPUT_LINKS_UNEXPECTED`는 Cargo 출력 디렉터리 밖의 별칭 등이 발견된 경우다. 외부 JAR·JRE 등 공급물에는 기존 단일 링크 검사를 적용한다.

생성한 실행 파일은 ELF64 little-endian x86_64·glibc 요구 버전을 검사한 뒤 `version`, package/CLI bundle build·verify에만 실행한다. `runtime/bin/java`와 전체 JRE의 ELF도 요구 버전을 검사하지만 Java·노드를 실행하지 않는다.

공개 릴리스와 해당 설치 CI는 [공통 빌드 스크립트](../tools/build-linux-release-cli.sh)로 Linux amd64용 CLI를 고정 Rust 1.86.0·glibc 2.31 이미지에서 링크한다. 노드 설치와 systemd 운용 대상은 Ubuntu 24.04다. 비공개 재전달 키트는 동봉한 Zig 링커로 glibc 2.34를 대상으로 CLI를 링크한다. 테스터가 Zig를 시스템에 설치할 필요는 없다. 현재 manifest의 `minGlibc`는 2.34다. ELF의 `VERSION_NEEDS`에서 더 높은 요구가 나오면 `GLIBC_REQUIREMENT_EXCEEDS_DECLARATION`으로 중단한다. 이 검사를 건너뛰거나 숫자만 바꾸지 않고 빌더/sysroot와 지원 정책을 다시 결정한다. 이 검사를 통과해도 모든 native dependency 또는 Ubuntu 이전 버전의 실행 호환성을 인수한 것은 아니다. JAR 내부 RocksDB native는 후속 실행 검증 대상이다.

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

## 5. 후보를 설치 입력으로 연결

빌드 이후에는 같은 장비에서 직접 만든 native verifier와 별도로 기록한 plan/receipt/CLI SHA-256을 사용한다. 받은 임의 실행 파일이나 receipt 자체를 실행 신뢰 근거로 삼지 않는다.

```sh
python3 -B tools/prepare-linux-operations.py \
  --kit /absolute/kit --plan-sha256 '<plan SHA-256>' \
  --built /absolute/candidate --receipt-sha256 '<receipt SHA-256>' \
  --cli /absolute/candidate/work/target/x86_64-unknown-linux-gnu/release/bxdl \
  --cli-sha256 '<직접 빌드한 CLI SHA-256>' \
  --output /absolute/new-operations --allow-unsigned-development
```

이 도구는 전체 archive·manifest·고정 JRE inventory·JAR·실행기 pin을 대조한 뒤 `runtime-package`와 `management-cli`를 새 사용자 폴더에 추출한다. 파일은 manifest의 mode/hash를 유지하고 symlink·hardlink·중복/불명 경로·기존 출력은 거절한다. `operations-plan.json`에는 공급물 identity와 독립 관리 CLI 설치, root helper 최초 배치 단계를 기록한다. sudo·JVM·노드·systemd를 실행하지 않으며 실제 설치나 profile 승인을 뜻하지 않는다.

시험용 입력은 다음처럼 준비한다. `instance` 이름은 새 시스템 노드 이름과 같아야 한다.

```sh
python3 -B tools/prepare-linux-test-inputs.py \
  --runtime /absolute/new-operations/runtime-package/runtime \
  --jar /absolute/new-operations/runtime-package/engine/nigo-node.jar \
  --output /absolute/new-test-inputs --instance linux-test-01 \
  --http-port 29680 --p2p-port 30680
```

선정 JRE257파일·RC.4 JAR·개발 helper class를 대조하고 일반 사용자로 고정 generator/keytool을 실행한다. 키는 시험 장비에서 새로 생성하며 결과는 private 폴더에 보관한다. `inputs/instance.json`, `inputs/node.json`, 체인과 private0600 credential 파일을 기존 `setup new`의 `--from`/`--native-config` 입력으로 사용한다. DB는 `/var/lib/bxdl/instances/<이름>/data`, 주소는 loopback, 로컬 목표는 `MANUAL_START_INITIALIZED`다. 4-validator 체인을 보존하되 한 노드의 자동 합의/sync/archive를 끄므로 정족수·거래 검증으로 계산하지 않는다.

두 도구는 sibling `build-linux-candidate.py`, `linux_build_contract.py`를 필요로 하며 operations 도구는 `receive-linux-candidate.py`도 필요하다. inputs 도구의 상위 `packaging/`에는 선정 Linux runtime/engine lock이 있어야 한다. private 전달본은 이 의존성과 외부 checksum을 함께 제공한다. 자세한 제품 명령과 권한은 [Linux 운용 안내](./linux-operations.md)를 따른다.

## 6. 패키지 고정 후 실제 시험

현재 Linux `setup`은 flat JRE/JAR 경로를 입력받고 관리자 helper를 고정 worker로 snapshot한다. [공개 다운로드 경로](./linux-release.md)의 Linux `bxdl install`은 외부 서명이 있는 runtime archive를 받는다. 이 도구로 만든 unsigned 후보는 비공개 키트의 추출 절차를 사용한다. 이 빌드 도구로 R-L archive를 만들었다고 Linux 일반 설치·profile 인수가 완료되는 것은 아니다. 다음 Linux 설치 시험의 root helper는 수신 검증한 실행기 A bytes에서 배치하고 해시를 확인해야 한다. 자세한 권한·수동 bootstrap은 [Linux 설치·운용](./linux-operations.md)을 따른다.

비공개 시험은 빌드·후보 검증을 사전 단계로 두고 새 Linux 설치 → 명시 init → systemd 수동 시작·로컬 READY·진단 → SSH 단절 유지 → 정상 종료·동일 DB 재시작까지 진행한다. 선정 JRE/엔진의 cold·native/RocksDB와 서비스 관측은 실제 실행의 근거로 구분한다. 별도 CLI 버전 전환·다중 노드·재부팅/자동 복구는 후속이다. 각 결과를 확보한 뒤에만 exact executor profile을 인수한다. 비공개 테스터 전달본은 기술 검증과 사용성 관측을 함께 진행하되 판정을 분리한다. 현재 새로운 NIGO 선행 구현 요청은 없다. Linux 실행에서 엔진/native 문제가 발견되면 고정된 RC.4 조합과 재현 자료로 별도 요청한다.
