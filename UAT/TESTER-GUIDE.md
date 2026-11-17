# BXDL Linux 빌드 검증 — 테스터 안내

이번 과업은 **전달받은 고정 소스를 Linux에서 빌드해 두 개발 패키지를 만드는 것**이다. 성공하면 런타임 패키지, 독립 관리 CLI 패키지, 빌드 기록을 반환한다. 설치·초기화·노드 서비스 실행은 이번 과업에 포함하지 않는다. 준비부터 반환까지 막힌 부분도 결과다.

## 1. 받은 파일과 환경 확인

전달 폴더에 다음 파일이 있어야 한다.

```text
build-kit.tar.gz
HANDOFF.json
SHA256SUMS
TESTER-GUIDE.md
TEST-RESULTS.md
IMPROVEMENTS.md
```

담당자에게 **키트 archive SHA-256, plan SHA-256, source commit** 세 값을 별도 전달 메시지로 받는다. `HANDOFF.json`의 `kitArchive.sha256`, `planSha256`, `sourceCommit`과 대조한다. 값이 다르거나 누락됐으면 실행하지 않고 담당자에게 알린다.

필요한 장비는 실제 Ubuntu 24.04, CPU x86_64, glibc, 일반 사용자 계정이다. WSL2·Mac·ARM64·root 계정은 이번 대상이 아니다. SSH로 자신의 장비에 접속해 진행할 수 있지만 접속 정보나 비밀번호를 담당자에게 보낼 필요는 없다. 빌드에는 sudo·systemd가 필요하지 않다.

터미널에서 아래를 확인한다. 운영체제·버전과 도구 버전만 결과 양식에 적고 사용자 이름·호스트 주소·개인 경로는 제외한다.

```sh
cat /etc/os-release
uname -m
uname -r
id -u
getconf GNU_LIBC_VERSION
python3 --version
rustup --version
cargo +1.86.0 --version
rustc +1.86.0 -vV
/usr/bin/cc --version
/usr/bin/readelf --version
```

`id -u`가 `0`이면 root이므로 중단하고 일반 사용자로 진행한다. `uname -r`에 `microsoft`가 포함되면 WSL이므로 이번 과업을 중단한다. 결과에는 숫자 UID·계정명 대신 일반 사용자 여부만 적는다.

Rust/Cargo는 **rustup의 1.86.0 toolchain**이어야 한다. rustup이 이미 있다면 아래 명령으로 준비할 수 있다. rustup이나 Python·C 컴파일러·binutils가 없으면 장비 관리자 또는 담당자에게 설치 지원을 요청하고, 요청한 내용과 시간을 기록한다.

```sh
rustup toolchain install 1.86.0 --profile minimal
```

Cargo 의존성 캐시가 없으면 빌드 중 네트워크가 필요하다. 사용자 또는 상위 경로의 Cargo config를 감지하면 도구가 중단할 수 있다. 기존 설정을 지우거나 검사를 우회하지 말고 깨끗한 빌드 환경을 담당자와 정한다. 새로운 작업 경로를 쓸 충분한 디스크 여유도 확인한다. 최초 환경 준비 시간과 실제 빌드 시간은 따로 기록한다.

## 2. 값 지정과 키트 확인

이하 명령은 **Bash** 기준이다. 필요한 경우 `bash`로 시작한다. `DELIVERY`는 받은 파일이 있는 실제 절대 경로로 바꾼다. `RUN`은 아직 없는 새 폴더여야 한다. 재시도는 `bxdl-uat-run-02`처럼 새 이름을 쓴다. 입력 archive나 실패한 작업 폴더를 덮어쓰지 않는다.

```bash
DELIVERY='/absolute/path/to/received-folder'
RUN="$HOME/bxdl-uat-run-01"
EXPECTED_KIT_SHA256='<담당자가 별도로 전달한 archive SHA-256>'
EXPECTED_PLAN_SHA256='<담당자가 별도로 전달한 plan SHA-256>'
EXPECTED_SOURCE_COMMIT='<담당자가 별도로 전달한 40자리 commit>'
```

아래 블록은 해시가 맞을 때만 새 작업 폴더를 만들고 키트를 푼다. `--same-permissions`는 키트에 고정된 파일 권한을 보존한다. 블록이 실패하면 다음 단계로 넘어가지 않는다.

```bash
(
  set -eu
  cd "$DELIVERY"
  printf '%s  %s\n' "$EXPECTED_KIT_SHA256" 'build-kit.tar.gz' | sha256sum --check -
  sha256sum --check SHA256SUMS
  mkdir -m 700 "$RUN"
  tar --no-same-owner --same-permissions -xzf build-kit.tar.gz -C "$RUN"
  python3 - "$RUN/bxdl-linux-build/build-plan.json" "$EXPECTED_PLAN_SHA256" "$EXPECTED_SOURCE_COMMIT" <<'PY'
import hashlib, json, pathlib, sys
raw = pathlib.Path(sys.argv[1]).read_bytes()
if hashlib.sha256(raw).hexdigest() != sys.argv[2]:
    raise SystemExit('PLAN_SHA256_MISMATCH')
plan = json.loads(raw)
if plan['source']['commit'] != sys.argv[3]:
    raise SystemExit('SOURCE_COMMIT_MISMATCH')
print('입력 plan·source commit 확인 완료')
PY
)
```

파일 손상·해시 불일치·기존 경로·권한 오류가 있으면 `TEST-RESULTS.md`에 단계와 오류를 적는다. `build-plan.json`이나 공급물 hash를 고쳐서 진행하지 않는다.

## 3. 빌드 실행과 시간 기록

시작 시각을 기록하고 아래 블록을 실행한다. 화면에는 빌드가 끝날 때까지 요약이 나타나지 않을 수 있다. Cargo 빌드 단계의 제한 시간은 30분이다. 전체 작업의 소요 시간도 별도로 기록한다.

```bash
KIT="$RUN/bxdl-linux-build"
date -u '+%Y-%m-%dT%H:%M:%SZ'
if python3 -B "$KIT/tools/build-linux-candidate.py" \
  --kit "$KIT" --plan-sha256 "$EXPECTED_PLAN_SHA256" \
  --output "$RUN/candidate" --allow-unsigned-development \
  > "$RUN/build-summary.json" 2> "$RUN/build-stderr.txt"; then
  BUILD_EXIT=0
else
  BUILD_EXIT=$?
fi
printf '%s\n' "$BUILD_EXIT" > "$RUN/build-exit-code.txt"
date -u '+%Y-%m-%dT%H:%M:%SZ'
cat "$RUN/build-summary.json"
printf '종료 코드: %s\n' "$BUILD_EXIT"
```

`outcome: PASS`와 **종료 코드 0**이 함께 나오면 빌드·패키징이 성공한 것이다. 실제 생성된 CLI는 version 및 package/CLI bundle build·verify에만 사용한다. Java·노드·systemd는 실행하지 않는다. 성공해도 설치·서비스 검증을 PASS로 표시하지 않는다.

실패 시 `reasonCode`를 결과 양식에 옮기고 중단한다. 예를 들어 지원 환경·입력 hash·도구 버전·glibc 요구 조건을 통과하지 못할 수 있다. 검사를 제거하거나 파일을 바꾸어 성공시키지 않는다. 재시도가 필요하면 원래 실패 기록을 보존하고 새 `RUN` 경로를 사용한다. `candidate/work/`의 원문 로그는 로컬에 보존하며 별도 요청이 있을 때 필요한 부분만 검토해 공유한다.

## 4. 결과와 개선 의견 작성

받은 원본 양식을 직접 바꾸지 않고 반환 폴더에 복사한다. 아래 블록은 성공·실패 모두 실행할 수 있다. `return`이 이미 존재하면 중단하므로 이전 기록을 덮어쓰지 않는다. 빌드 전에 중단했다면 2단계의 `DELIVERY`·`RUN` 경로 변수만 지정한다. `RUN` 폴더가 아직 없으면 `mkdir -m 700 "$RUN"`으로 새 기록 폴더를 만든 뒤 진행한다.

```bash
(
  set -eu
  mkdir -m 700 "$RUN/return"
  cp "$DELIVERY/TEST-RESULTS.md" "$RUN/return/TEST-RESULTS.md"
  cp "$DELIVERY/IMPROVEMENTS.md" "$RUN/return/IMPROVEMENTS.md"
  cp "$DELIVERY/HANDOFF.json" "$RUN/return/HANDOFF.json"
  for FILE in build-summary.json build-exit-code.txt; do
    if [ -f "$RUN/$FILE" ]; then cp "$RUN/$FILE" "$RUN/return/$FILE"; fi
  done
)
```

빌드 전에 중단돼 요약 파일이 없으면 위 블록은 양식 두 개와 `HANDOFF.json`만 복사한다. 결과 양식에 `BLOCKED`와 중단 단계를 적고 실행하지 않은 단계는 `NOT_RUN`으로 남긴다.

`TEST-RESULTS.md`와 `IMPROVEMENTS.md`를 편집기로 작성한다. 종료 코드·오류 코드·소요 시간·도움받은 내용·기대와 실제 결과를 적는다. 좋았던 점도 구체적으로 적을 수 있다. 스크린샷은 선택 사항이며 개인 정보와 비밀이 보이면 먼저 가린다. `build-stderr.txt`는 원문에 개인 경로가 있을 수 있으므로 자동 복사하지 않는다. 필요한 오류 문구만 검토해 결과 양식에 옮긴다.

## 5. 반환 파일 모으기

**성공했을 때** 아래 세 파일을 반환 폴더에 넣는다. 키트 자체나 `candidate/work` 전체를 반환할 필요는 없다.

```bash
(
  set -eu
  cp "$RUN/candidate/runtime.tar.gz" "$RUN/return/runtime.tar.gz"
  cp "$RUN/candidate/management-cli.tar.gz" "$RUN/return/management-cli.tar.gz"
  cp "$RUN/candidate/receipt.json" "$RUN/return/receipt.json"
)
```

**실패했을 때**는 두 패키지를 성공 결과로 보내지 않는다. `candidate/receipt.json`이 생성됐으면 해당 실패 기록만 반환 폴더에 복사한다. 없으면 결과 양식에 `receipt 미생성`을 적는다. 파일이 없는 것을 채우기 위해 빈 receipt나 임의 PASS 기록을 만들지 않는다.

```bash
if [ -f "$RUN/candidate/receipt.json" ] && [ ! -f "$RUN/return/receipt.json" ]; then
  cp "$RUN/candidate/receipt.json" "$RUN/return/receipt.json"
fi
```

양식 작성을 마친 뒤 반환 폴더만 압축한다. 명령은 `return`에 있는 일반 파일들의 checksum을 기록한다. 작성한 내용에 사용자 이름·주소·비밀번호·키가 남지 않았는지 확인한 뒤 실행한다.

```bash
(
  set -eu
  cd "$RUN/return"
  sha256sum HANDOFF.json TEST-RESULTS.md IMPROVEMENTS.md > SHA256SUMS
  for FILE in build-summary.json build-exit-code.txt receipt.json runtime.tar.gz management-cli.tar.gz; do
    if [ -f "$FILE" ]; then sha256sum "$FILE" >> SHA256SUMS; fi
  done
  cd "$RUN"
  test ! -e return.tar.gz
  tar -czf return.tar.gz return
  sha256sum return.tar.gz
  if [ -f return/receipt.json ]; then sha256sum return/receipt.json; fi
)
```

**반환 방법:** 담당자와 정한 파일 전달 채널로 `return.tar.gz`를 보낸다. 같은 전달 메시지에 결과 요약(PASS/FAIL/BLOCKED), source commit, archive SHA-256, receipt SHA-256(존재할 때)을 텍스트로 적는다. 수신 담당자가 별도로 해시를 대조할 수 있도록 한다. 개인 SSH 정보나 원문 작업 폴더를 보낼 필요는 없다.

반환 후 로컬 키트와 실패 원자료는 담당자가 수신 확인할 때까지 보관한다. 다음 설치·운용 과업은 이번 산출물 검토 결과와 별도 안내를 받은 뒤 진행한다.
