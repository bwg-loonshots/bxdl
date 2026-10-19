# 선정 JRE 준비

첫 개발 후보는 **Eclipse Temurin21.0.12.1+1-LTS, macOS arm64 전체 JRE**다.
[선정 이유·책임 분담](../design/2026-10-19-jre-selection.md), [공급 snapshot](../contracts/runtime/temurin-21.0.12.1-macos-arm64/README.md)을 따른다.
사용자 설치 흐름에는 JRE 다운로드/선택 단계를 추가하지 않는다. 아래는 패키지 제작자용이다.

## 1. 공급 입력 준비

신뢰한 저장소의 `packaging/runtime-lock.temurin-21-macos-arm64.json`을 선택한다.
`archive.url`에서 받은 원본의 크기와 SHA256을 **먼저** lock의 값과 대조한다. 공식 체크섬/API의 값도 대조한다.
lock을 검증하지 않은 다운로드 파일에서 자동 생성하면 출처를 인증하지 못한다. 버전 URL만 믿고 latest를 추적하지 않는다.

검증한 공급 archive를 별도 새 private 폴더에 안전하게 추출한다. 선택한 archive는 directory/regular entry만 있으며
전체328 entry,262파일이다. 이 중 Home259파일을 준비하며 외곽3파일의 제외 내역은 snapshot의 `observed-runtime.json`에 보존한다. 대상 Home은 `jdk-21.0.12.1+1-jre/Contents/Home`이다.
이 명령은 tar 추출기가 아니므로 외부 입력을 먼저 임의 경로에 풀지 않는다.

## 2. Rust로 준비 폴더 생성

```sh
bxdl runtime stage \
  --source ./upstream/jdk-21.0.12.1+1-jre/Contents/Home \
  --lock ./packaging/runtime-lock.temurin-21-macos-arm64.json \
  --output ./new-stage --json
```

새 `new-stage/runtime/`와 `new-stage/docs/runtime-provenance.json`을 만든다. 전체 파일 목록·내용과 실행 mode를 검증한다.
출력은 기존 경로를 덮어쓰지 않는다. 실패하면 원인을 확인하고 별도 새 경로로 재시도한다. 자동 삭제/resume은 제공하지 않는다.
결과의 archiveVerification/runtimeExecution은 NOT_PERFORMED다. 성공은 입력 lock과 runtime 파일 일치이며
JRE의 공급 서명, 엔진 호환성, source archive 수신 또는 완성 제품 출시 검증을 뜻하지 않는다.

## 3. 패키지 조립

같은 stage에 Rust CLI `bin/bxdl`, 수신 RC1 `engine/nigo-node.jar`를 준비하고
`package-spec.rc1-temurin.example.json`과 `engine-lock.rc1-temurin-development.json`을 사용한다.
후자는 실제 JAR/Java hash를 채운 development lock이다. 별도 제품 revision은 빌드할 CLI에 맞게 설정한다.

- `packaging/runtime-notices.temurin-21.txt` → `licenses/THIRD_PARTY_NOTICES`
- `packaging/runtime-bom.temurin-21.cdx.json` → `licenses/SBOM.json` (현재 runtime-only 범위를 유지)
- runtime snapshot의 공급 metadata/관측/source-supply 문서 → `docs/runtime/`
- 원본 공급 SBOM → `licenses/temurin-supplier-sbom.json`

이 자료는 runtime 부분의 개발용 안내다. 전체 제품 dependency 목록/NOTICE가 완성된 것으로 취급하지 않는다.
원본 runtime/legal 디렉터리는 staging 도구가 함께 보존한다. 고객 출시에 필요한 대응 소스는
source-supply.json의 고정 입력과 함께 별도 배포 kit에 포함하거나 binary와 동등한 접근 경로로 제공한다.

이후 기존 `package build/verify`로 외부 Ed25519 key 서명과 전체 파일 검증을 수행한다.
서명 key는 stage 밖에 둔다. `setup --install`의 패키지·lock 입력과 일반 설치·초기화·기동 흐름은 그대로다.

## 검증 경계

full JRE에 필요한 알려진 모듈이 포함되어도 NIGO 전체 호환성 확인을 대신하지 않는다.
RC1 JAR를 그대로 사용한 소비자 실제 lifecycle과 공급자 engine/crypto/QBFT 검증을 각각 기록한다.
JRE를 교체하면 같은 JAR여도 새 제품 패키지의 영향 범위를 다시 시험한다.
원본 macOS bundle 서명, Apple notarization, 지원 최소 OS, Linux/Docker 지원은 여기서 확정하지 않는다.
